import type { WorkspaceDb } from '../db/schema';
import type { ResolvedItem } from '../sets/useResolvedSet';
import { formatKey } from '../chart/notes';
import { DEFAULT_THEME, type SessionState, type Theme } from './session';
import { buildSlides, type Snapshot, type SnapshotItem } from './slides';

/**
 * Where a session lives while it is running.
 *
 * Local only, on purpose: a session has no meaning after it ends and none at all on another
 * device, and it must run with the internet unplugged. It is written to IndexedDB on every
 * revision so that a control window closed by accident can be offered back.
 */

/** How long a session stays resumable after its control window disappears. */
export const RESUME_WINDOW_MS = 60_000;

/** The last twenty sessions are kept; older ones are dropped as new ones start. */
export const LOG_LIMIT = 20;

/**
 * Freezes the set as it is right now (acceptance criterion 2). Everything the outputs will ever
 * need is copied in: edits made elsewhere during the service cannot reach the screen.
 */
export function takeSnapshot(setId: string | null, setName: string, items: ResolvedItem[]): Snapshot {
  return {
    setId,
    setName,
    takenAt: new Date().toISOString(),
    items: items.map((resolved): SnapshotItem => ({
      itemId: resolved.item.id,
      songId: resolved.song?.id ?? null,
      title: resolved.title,
      body: resolved.arrangement?.body ?? null,
      writtenKey: resolved.written === null ? null : formatKey(resolved.written),
      setKey: resolved.item.key_override,
      capo: resolved.capo,
      itemType: resolved.item.item_type,
      content: resolved.item.content,
      note: resolved.item.note,
      sheetId: null,
      sheetPages: null,
    })),
  };
}

export async function createSession(
  db: WorkspaceDb,
  workspaceId: string,
  snapshot: Snapshot,
  theme: Theme = DEFAULT_THEME,
): Promise<SessionState> {
  const state: SessionState = {
    session_id: crypto.randomUUID(),
    workspace_id: workspaceId,
    set_snapshot: snapshot,
    slides: buildSlides(snapshot, theme.font_size_vh, theme.safe_area_pct),
    index: 0,
    blank_mode: 'none',
    message: null,
    stage_message: null,
    theme,
    started_at: new Date().toISOString(),
    revision: 1,
    ended: false,
  };

  await save(db, state);

  await db.session_log.put({
    session_id: state.session_id,
    set_id: snapshot.setId,
    set_name: snapshot.setName,
    started_at: state.started_at,
    ended_at: null,
    song_ids: snapshot.items.map((item) => item.songId).filter((id): id is string => id !== null),
    events: [],
  });

  await trimLog(db);

  return state;
}

export async function save(db: WorkspaceDb, state: SessionState): Promise<void> {
  await db.live_sessions.put({
    session_id: state.session_id,
    state,
    updated_at: new Date().toISOString(),
  });
}

export async function load(db: WorkspaceDb, sessionId: string): Promise<SessionState | null> {
  const record = await db.live_sessions.get(sessionId);

  return (record?.state as SessionState | undefined) ?? null;
}

/**
 * A session that was running moments ago and has no control window any more. Offered back
 * rather than resumed silently — the operator may have ended the service and closed the laptop.
 */
export async function resumable(db: WorkspaceDb, now = Date.now()): Promise<SessionState | null> {
  const records = await db.live_sessions.toArray();

  for (const record of records.sort((a, b) => b.updated_at.localeCompare(a.updated_at))) {
    const state = record.state as SessionState;

    if (! state.ended && now - Date.parse(record.updated_at) < RESUME_WINDOW_MS) {
      return state;
    }
  }

  return null;
}

export async function logAdvance(db: WorkspaceDb, state: SessionState): Promise<void> {
  const entry = await db.session_log.get(state.session_id);

  if (entry === undefined) {
    return;
  }

  await db.session_log.update(state.session_id, {
    events: [
      ...entry.events,
      {
        at: new Date().toISOString(),
        index: state.index,
        title: state.slides[state.index]?.songTitle ?? '',
      },
    ].slice(-500),
  });
}

export async function endSession(db: WorkspaceDb, state: SessionState): Promise<void> {
  await save(db, { ...state, ended: true, revision: state.revision + 1 });
  await db.session_log.update(state.session_id, { ended_at: new Date().toISOString() });
  await db.live_sessions.delete(state.session_id);
}

async function trimLog(db: WorkspaceDb): Promise<void> {
  const entries = (await db.session_log.toArray()).sort((a, b) => b.started_at.localeCompare(a.started_at));

  for (const entry of entries.slice(LOG_LIMIT)) {
    await db.session_log.delete(entry.session_id);
  }
}

/**
 * When each song was last played, from the local session log. The library sorts by it, and it
 * is deliberately per device: it is a memory, not a record the band shares.
 */
export async function lastPlayed(db: WorkspaceDb): Promise<Map<string, string>> {
  const played = new Map<string, string>();

  for (const entry of await db.session_log.toArray()) {
    for (const songId of entry.song_ids) {
      const previous = played.get(songId);

      if (previous === undefined || previous < entry.started_at) {
        played.set(songId, entry.started_at);
      }
    }
  }

  return played;
}
