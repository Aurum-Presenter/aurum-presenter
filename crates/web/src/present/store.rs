//! Where a session lives while it is running.
//!
//! Local only, on purpose: a session has no meaning after it ends and none at all on another
//! device, and it must run with the internet unplugged. It is written to IndexedDB on every
//! revision so that a control window closed by accident can be offered back.

use std::collections::HashMap;

use aurum_core::present::session::{SessionState, Theme};
use aurum_core::present::slides::{Snapshot, SnapshotItem};

use crate::db::records::{LiveSession, SessionLogEntry, SessionLogEvent};
use crate::db::{Database, DbError};
use crate::sets::ResolvedItem;

/// How long a session stays resumable after its control window disappears.
pub const RESUME_WINDOW_MS: i64 = 60_000;

/// The last twenty sessions are kept; older ones are dropped as new ones start.
pub const LOG_LIMIT: usize = 20;

/// The most events one session's log will hold. Long enough for a three-hour carol service.
const EVENT_LIMIT: usize = 500;

/// Freezes the set as it is right now (acceptance criterion 2).
///
/// Everything the outputs will ever need is copied in: edits made elsewhere during the service
/// cannot reach the screen. That is the whole promise — nobody retitles a song mid-verse.
pub fn take_snapshot(set_id: Option<&str>, set_name: &str, items: &[ResolvedItem]) -> Snapshot {
    Snapshot {
        set_id: set_id.map(str::to_owned),
        set_name: set_name.to_owned(),
        taken_at: crate::now(),
        items: items
            .iter()
            .map(|resolved| SnapshotItem {
                item_id: resolved.item.id.clone(),
                song_id: resolved.song.as_ref().map(|song| song.id.clone()),
                title: resolved.title.clone(),
                body: resolved.arrangement.as_ref().map(|row| row.body.clone()),
                written_key: resolved.written.map(|key| key.to_string()),
                set_key: resolved.item.key_override.clone(),
                capo: resolved.capo as i16,
                item_type: resolved.item.item_type.clone(),
                content: resolved.item.content.clone(),
                note: resolved.item.note.clone(),
                sheet_id: None,
                sheet_pages: None,
            })
            .collect(),
    }
}

#[derive(Clone)]
pub struct Sessions {
    db: Database,
}

impl Sessions {
    pub fn new(db: Database) -> Sessions {
        Sessions { db }
    }

    pub async fn create(
        &self,
        workspace_id: &str,
        snapshot: Snapshot,
        theme: Theme,
    ) -> Result<SessionState, DbError> {
        let slides = snapshot.slides(theme.font_size_vh, theme.safe_area_pct);

        let state = SessionState {
            session_id: crate::new_id(),
            workspace_id: workspace_id.to_owned(),
            slides,
            index: 0,
            blank_mode: Default::default(),
            message: None,
            stage_message: None,
            theme,
            started_at: crate::now(),
            revision: 1,
            ended: false,
            set_snapshot: snapshot,
        };

        self.save(&state).await?;

        self.db
            .put(
                "session_log",
                &SessionLogEntry {
                    session_id: state.session_id.clone(),
                    set_id: state.set_snapshot.set_id.clone(),
                    set_name: state.set_snapshot.set_name.clone(),
                    started_at: state.started_at.clone(),
                    ended_at: None,
                    song_ids: state
                        .set_snapshot
                        .items
                        .iter()
                        .filter_map(|item| item.song_id.clone())
                        .collect(),
                    events: Vec::new(),
                },
            )
            .await?;

        self.trim_log().await?;

        Ok(state)
    }

    /// Written on every revision, quietly: the outputs hear about a change over the transport,
    /// not by watching the database, and waking every query in the tab mid-service is exactly
    /// the kind of work a stage machine cannot spare.
    pub async fn save(&self, state: &SessionState) -> Result<(), DbError> {
        self.db
            .put_quietly(
                "live_sessions",
                &LiveSession {
                    session_id: state.session_id.clone(),
                    state: serde_json::to_value(state).unwrap_or_default(),
                    updated_at: crate::now(),
                },
            )
            .await?;

        Ok(())
    }

    pub async fn load(&self, session_id: &str) -> Option<SessionState> {
        let held: LiveSession = self
            .db
            .get("live_sessions", session_id)
            .await
            .ok()
            .flatten()?;

        serde_json::from_value(held.state).ok()
    }

    /// A session that was running moments ago and has no control window any more.
    ///
    /// Offered back rather than resumed silently — the operator may have ended the service and
    /// closed the laptop, and a projector that lights up on its own is worse than one that asks.
    pub async fn resumable(&self, now_ms: i64) -> Option<SessionState> {
        let mut held: Vec<LiveSession> = self.db.all("live_sessions").await.ok()?;

        held.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

        held.into_iter()
            .filter_map(|record| {
                let at = aurum_core::time::parse(&record.updated_at)?;
                let state: SessionState = serde_json::from_value(record.state).ok()?;

                (!state.ended && now_ms - at < RESUME_WINDOW_MS).then_some(state)
            })
            .next()
    }

    pub async fn log_advance(&self, state: &SessionState) -> Result<(), DbError> {
        let Some(mut entry): Option<SessionLogEntry> =
            self.db.get("session_log", &state.session_id).await?
        else {
            return Ok(());
        };

        entry.events.push(SessionLogEvent {
            at: crate::now(),
            index: state.index,
            title: state
                .slides
                .get(state.index)
                .map(|slide| slide.song_title.clone())
                .unwrap_or_default(),
        });

        if entry.events.len() > EVENT_LIMIT {
            entry.events.drain(..entry.events.len() - EVENT_LIMIT);
        }

        self.db.put_quietly("session_log", &entry).await?;

        Ok(())
    }

    pub async fn end(&self, state: &SessionState) -> Result<(), DbError> {
        let ended = SessionState {
            ended: true,
            revision: state.revision + 1,
            ..state.clone()
        };

        self.save(&ended).await?;

        if let Some(mut entry) = self
            .db
            .get::<SessionLogEntry>("session_log", &state.session_id)
            .await?
        {
            entry.ended_at = Some(crate::now());
            self.db.put_quietly("session_log", &entry).await?;
        }

        self.db
            .delete("live_sessions", &state.session_id.as_str().into())
            .await
    }

    async fn trim_log(&self) -> Result<(), DbError> {
        let mut entries: Vec<SessionLogEntry> = self.db.all("session_log").await?;

        entries.sort_by(|left, right| right.started_at.cmp(&left.started_at));

        for entry in entries.into_iter().skip(LOG_LIMIT) {
            self.db
                .delete("session_log", &entry.session_id.as_str().into())
                .await?;
        }

        Ok(())
    }

    /// When each song was last played, from the local session log.
    ///
    /// The library sorts by it, and it is deliberately per device: it is a memory, not a record
    /// the band shares.
    pub async fn last_played(&self) -> HashMap<String, String> {
        let entries: Vec<SessionLogEntry> = self.db.all("session_log").await.unwrap_or_default();
        let mut played: HashMap<String, String> = HashMap::new();

        for entry in entries {
            for song_id in entry.song_ids {
                let seen = played
                    .entry(song_id)
                    .or_insert_with(|| entry.started_at.clone());

                if *seen < entry.started_at {
                    *seen = entry.started_at.clone();
                }
            }
        }

        played
    }
}
