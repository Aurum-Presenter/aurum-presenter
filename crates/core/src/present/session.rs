//! The one object every screen in a session renders from.
//!
//! There is exactly one writer — the control surface — and every output is a pure function of
//! what it last received. That is what lets an audience window be reloaded mid-service and come
//! back on the right slide without asking anyone anything.

use serde::{Deserialize, Serialize};

use super::slides::{Slide, Snapshot};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlankMode {
    #[default]
    None,
    Black,
    Logo,
    Freeze,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundKind {
    #[default]
    Color,
    Gradient,
    Image,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    #[default]
    Center,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub font_family: String,
    /// Maximum size; the fitting pass may come down from it.
    pub font_size_vh: f64,
    pub text_color: String,
    pub background_kind: BackgroundKind,
    pub background_value: String,
    pub align: Align,
    pub safe_area_pct: f64,
    pub show_section_labels: bool,
}

impl Default for Theme {
    fn default() -> Theme {
        Theme {
            id: "default".to_owned(),
            name: "Default".to_owned(),
            font_family: "system-ui, sans-serif".to_owned(),
            font_size_vh: 8.0,
            text_color: "#ffffff".to_owned(),
            background_kind: BackgroundKind::Color,
            background_value: "#000000".to_owned(),
            align: Align::Center,
            safe_area_pct: 5.0,
            show_section_labels: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub workspace_id: String,
    pub set_snapshot: Snapshot,
    pub slides: Vec<Slide>,
    pub index: usize,
    pub blank_mode: BlankMode,
    /// An overlay for the audience only — "the service will begin in five minutes".
    pub message: Option<String>,
    /// A note for the stage views only — "two more times", "wrap it up".
    pub stage_message: Option<String>,
    pub theme: Theme,
    pub started_at: String,
    /// Monotonic. An output ignores anything it receives with a lower revision.
    pub revision: u64,
    pub ended: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputKind {
    Audience,
    Stage,
    PairedStage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputStatus {
    pub output_id: String,
    pub kind: OutputKind,
    pub label: String,
    pub joined_at: String,
    pub last_ack_revision: u64,
    pub responding: bool,
    pub can_advance: bool,
}

/// Messages on the wire, whichever wire it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SessionMessage {
    State {
        state: Box<SessionState>,
    },
    Ack {
        output_id: String,
        kind: OutputKind,
        label: String,
        revision: u64,
    },
    Hello {
        output_id: String,
        kind: OutputKind,
        label: String,
    },
    RequestState {
        output_id: String,
    },
    Advance {
        output_id: String,
        delta: i64,
    },
    Bye {
        output_id: String,
    },
}

pub fn channel_name(session_id: &str) -> String {
    format!("aurum-session-{session_id}")
}

impl SessionState {
    fn last_index(&self) -> usize {
        self.slides.len().saturating_sub(1)
    }

    /// Moving past either end is not a move, and does not bump the revision — an output that
    /// re-rendered on every held-down arrow key would flicker for no reason.
    pub fn advanced(&self, delta: i64) -> SessionState {
        let index = (self.index as i64 + delta).clamp(0, self.last_index() as i64) as usize;

        if index == self.index {
            return self.clone();
        }

        SessionState {
            index,
            revision: self.revision + 1,
            ..self.clone()
        }
    }

    pub fn jumped(&self, index: i64) -> SessionState {
        SessionState {
            index: index.clamp(0, self.last_index() as i64) as usize,
            revision: self.revision + 1,
            ..self.clone()
        }
    }

    /// Asking for the mode that is already on turns it off — one key both blanks and unblanks.
    pub fn blanked(&self, mode: BlankMode) -> SessionState {
        SessionState {
            blank_mode: if self.blank_mode == mode {
                BlankMode::None
            } else {
                mode
            },
            revision: self.revision + 1,
            ..self.clone()
        }
    }

    pub fn with_message(&self, message: Option<&str>) -> SessionState {
        SessionState {
            message: non_empty(message),
            revision: self.revision + 1,
            ..self.clone()
        }
    }

    pub fn with_stage_message(&self, message: Option<&str>) -> SessionState {
        SessionState {
            stage_message: non_empty(message),
            revision: self.revision + 1,
            ..self.clone()
        }
    }

    /// The slide the audience should be showing, which is not always the current one.
    pub fn audience_slide(&self) -> Option<&Slide> {
        match self.blank_mode {
            BlankMode::Black | BlankMode::Logo => None,
            _ => self.slides.get(self.index),
        }
    }

    /// The slide the stage should be showing. Blanking the audience never blanks the stage — the
    /// band still needs the words while the congregation looks at a logo (acceptance criterion 4).
    pub fn stage_slide(&self) -> Option<&Slide> {
        self.slides.get(self.index)
    }

    pub fn next_slide(&self) -> Option<&Slide> {
        self.slides.get(self.index + 1)
    }
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.filter(|text| !text.is_empty()).map(str::to_owned)
}

/// Codes avoid characters that are misread on a dark stage: no 0/O, no 1/I.
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Six characters from six random bytes. The randomness comes from the caller because this crate
/// has no clock and no entropy of its own — the shell holds both.
pub fn pairing_code(bytes: [u8; 6]) -> String {
    bytes
        .iter()
        .map(|byte| ALPHABET[usize::from(*byte) % ALPHABET.len()] as char)
        .collect()
}

pub const CODE_TTL_MS: i64 = 30 * 60 * 1000;

pub fn normalise_code(input: &str) -> String {
    input
        .trim()
        .to_uppercase()
        .chars()
        .filter(|character| character.is_ascii_uppercase() || ('2'..='9').contains(character))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeLife {
    pub expired: bool,
    pub minutes_left: i64,
}

/// What is left of a pairing code. A code is a shared secret for one session, so the control
/// surface stops listening when it runs out rather than only saying that it is old.
pub fn code_life(expires_ms: i64, at_ms: i64) -> CodeLife {
    CodeLife {
        expired: at_ms >= expires_ms,
        minutes_left: (((expires_ms - at_ms) as f64) / 60_000.0).ceil().max(1.0) as i64,
    }
}

pub fn is_valid_code(input: &str) -> bool {
    let code = normalise_code(input);

    code.len() == 6 && code.bytes().all(|byte| ALPHABET.contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slide(id: &str) -> Slide {
        Slide {
            id: id.to_owned(),
            item_id: "item".to_owned(),
            song_title: "Song".to_owned(),
            written_key: Some("G".to_owned()),
            ..Slide::default()
        }
    }

    fn state() -> SessionState {
        SessionState {
            session_id: "session".to_owned(),
            workspace_id: "workspace".to_owned(),
            slides: vec![slide("a"), slide("b"), slide("c")],
            started_at: "2026-09-06T09:00:00Z".to_owned(),
            revision: 1,
            ..SessionState::default()
        }
    }

    #[test]
    fn moves_and_bumps_the_revision_which_outputs_order_themselves_by() {
        let next = state().advanced(1);

        assert_eq!(next.index, 1);
        assert_eq!(next.revision, 2);
    }

    #[test]
    fn stops_at_both_ends_without_bumping_the_revision_for_a_move_that_does_nothing() {
        assert_eq!(state().advanced(-1), state());

        let at_end = SessionState {
            index: 2,
            ..state()
        };
        assert_eq!(at_end.advanced(1), at_end);
        assert_eq!(state().advanced(99).index, 2);
    }

    #[test]
    fn jumps_to_a_slide_clamped_to_the_list() {
        assert_eq!(state().jumped(2).index, 2);
        assert_eq!(state().jumped(99).index, 2);
        assert_eq!(state().jumped(-5).index, 0);
    }

    /// Acceptance criterion 4: blanking the audience must not blank the stage.
    #[test]
    fn blanks_the_audience_and_leaves_the_stage_showing_the_words() {
        let blanked = state().blanked(BlankMode::Black);

        assert!(blanked.audience_slide().is_none());
        assert_eq!(
            blanked.stage_slide().map(|slide| slide.id.as_str()),
            Some("a")
        );
        assert_eq!(
            blanked.next_slide().map(|slide| slide.id.as_str()),
            Some("b")
        );
    }

    #[test]
    fn toggles_the_same_blank_mode_off_again() {
        assert_eq!(
            state()
                .blanked(BlankMode::Logo)
                .blanked(BlankMode::Logo)
                .blank_mode,
            BlankMode::None
        );
    }

    #[test]
    fn freezing_leaves_the_audience_on_the_slide_it_is_on() {
        assert_eq!(
            state()
                .blanked(BlankMode::Freeze)
                .audience_slide()
                .map(|slide| slide.id.as_str()),
            Some("a")
        );
    }

    #[test]
    fn keeps_audience_and_stage_messages_apart() {
        let with_both = state()
            .with_message(Some("Starting soon"))
            .with_stage_message(Some("two more times"));

        assert_eq!(with_both.message.as_deref(), Some("Starting soon"));
        assert_eq!(with_both.stage_message.as_deref(), Some("two more times"));
        assert_eq!(with_both.with_message(Some("")).message, None);
    }

    /// An empty session must not panic on the arrow keys the operator is already pressing.
    #[test]
    fn a_session_with_no_slides_still_takes_the_arrow_keys() {
        let empty = SessionState::default();

        assert_eq!(empty.advanced(1).index, 0);
        assert_eq!(empty.jumped(4).index, 0);
        assert!(empty.stage_slide().is_none());
    }

    #[test]
    fn codes_avoid_characters_that_are_misread_on_a_dark_stage() {
        for attempt in 0..=u8::MAX {
            let code = pairing_code([attempt; 6]);

            assert_eq!(code.len(), 6);
            assert!(!code.contains(['0', '1', 'O', 'I']));
            assert!(is_valid_code(&code));
        }
    }

    #[test]
    fn runs_out_after_half_an_hour_and_says_how_long_is_left() {
        let issued = 1_757_152_800_000;
        let expires = issued + CODE_TTL_MS;

        assert!(!code_life(expires, issued).expired);
        assert_eq!(code_life(expires, issued).minutes_left, 30);
        assert_eq!(
            code_life(expires, issued + 29 * 60_000 + 30_000).minutes_left,
            1
        );
        assert!(!code_life(expires, expires - 1).expired);
        assert!(code_life(expires, expires).expired);
        assert!(code_life(expires, expires + 60_000).expired);
    }

    #[test]
    fn accepts_what_a_person_types_in_any_case_with_spaces() {
        assert_eq!(normalise_code(" 4kj9qp "), "4KJ9QP");
        assert!(is_valid_code("4kj-9qp"));
        assert!(!is_valid_code("4KJ9Q"));
        assert!(!is_valid_code("4KJ9Q0"));
    }

    #[test]
    fn names_the_channel_after_the_session() {
        assert_eq!(channel_name("abc"), "aurum-session-abc");
    }
}
