import type { SetItem, SetItemType, SetRecord, Song, WorkspaceDb } from '../db/schema';
import { uuidv7 } from '../db/uuid';
import type { SyncEngine } from '../sync/engine';
import { byRank, rankBetween, rankForMove } from './rank';

/**
 * Sets: the running order for one service or gig.
 *
 * Every write here is local-first like the rest of the app. Ordering uses fractional ranks, so
 * adding an item touches one row and reordering touches exactly the row that moved.
 */

export interface SetInput {
  name: string;
  scheduled_for?: string | null;
  venue?: string | null;
  notes?: string | null;
  assigned_members?: string[];
}

/** The window in which a set pins itself for offline use, business rule 5. */
export const AUTO_PIN_DAYS = 14;

export class Sets {
  constructor(
    private readonly db: WorkspaceDb,
    private readonly engine: SyncEngine,
  ) {}

  async create(input: SetInput): Promise<string> {
    const id = uuidv7();

    await this.engine.record('sets', id, 'upsert', {
      name: input.name.trim(),
      scheduled_for: blank(input.scheduled_for),
      venue: blank(input.venue),
      notes: blank(input.notes),
      assigned_members: JSON.stringify(input.assigned_members ?? []),
      pinned: 0,
    });

    return id;
  }

  async update(id: string, changes: Record<string, unknown>): Promise<void> {
    await this.engine.record('sets', id, 'upsert', changes);
  }

  async remove(id: string): Promise<void> {
    await this.engine.record('sets', id, 'delete', {});
  }

  /** Business rule 6: items and overrides come along, the date does not. */
  async duplicate(id: string): Promise<string | null> {
    const original = await this.db.sets.get(id);

    if (original === undefined) {
      return null;
    }

    const copy = await this.create({
      name: `${original.name} (copy)`,
      venue: original.venue,
      notes: original.notes,
      assigned_members: parseList(original.assigned_members),
    });

    for (const item of await this.items(id)) {
      await this.engine.record('set_items', uuidv7(), 'upsert', {
        set_id: copy,
        rank: item.rank,
        song_id: item.song_id,
        item_type: item.item_type,
        content: item.content,
        title_snapshot: item.title_snapshot,
        key_override: item.key_override,
        capo_override: item.capo_override,
        arrangement_id: item.arrangement_id,
        sheet_part_override: item.sheet_part_override,
        sections: item.sections,
        note: item.note,
      });
    }

    return copy;
  }

  async items(setId: string): Promise<SetItem[]> {
    return byRank(
      await this.db.set_items
        .where('set_id').equals(setId)
        .filter((row) => row.deleted_at === null)
        .toArray(),
    );
  }

  /** Appends songs in the order they were selected, each with the title it had at the time. */
  async addSongs(setId: string, songs: Song[]): Promise<void> {
    let last = (await this.items(setId)).at(-1)?.rank ?? null;

    for (const song of songs) {
      last = rankBetween(last, null);

      await this.engine.record('set_items', uuidv7(), 'upsert', {
        set_id: setId,
        rank: last,
        song_id: song.id,
        title_snapshot: song.title,
      });
    }
  }

  async addItem(setId: string, type: SetItemType, content: string): Promise<void> {
    const last = (await this.items(setId)).at(-1)?.rank ?? null;

    await this.engine.record('set_items', uuidv7(), 'upsert', {
      set_id: setId,
      rank: rankBetween(last, null),
      item_type: type,
      content,
    });
  }

  async move(setId: string, from: number, to: number): Promise<void> {
    const items = await this.items(setId);
    const rank = rankForMove(items.map((item) => item.rank), from, to);

    if (rank === null) {
      return;
    }

    await this.engine.record('set_items', items[from]!.id, 'upsert', { rank });
  }

  async updateItem(id: string, changes: Record<string, unknown>): Promise<void> {
    await this.engine.record('set_items', id, 'upsert', changes);
  }

  async removeItem(id: string): Promise<void> {
    await this.engine.record('set_items', id, 'delete', {});
  }
}

function blank(value: string | null | undefined): string | null {
  return value == null || value.trim() === '' ? null : value.trim();
}

export function parseList(json: string | null): string[] {
  if (json === null) {
    return [];
  }

  try {
    const parsed = JSON.parse(json) as unknown;

    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string') : [];
  } catch {
    return [];
  }
}

/**
 * Business rule 5: a set is kept offline when it is explicitly pinned, or when its date falls
 * inside the window — including today, and including a set that has just happened, because a
 * band still reads last night's set on the way home.
 */
export function isAutoPinned(set: Pick<SetRecord, 'pinned' | 'scheduled_for'>, now = new Date()): boolean {
  if (set.pinned === 1) {
    return true;
  }

  if (set.scheduled_for === null) {
    return false;
  }

  const when = Date.parse(set.scheduled_for);

  if (Number.isNaN(when)) {
    return false;
  }

  // Whole days, not hours: a set is "today" all day, and yesterday's set is one day old at
  // breakfast as well as at midnight.
  const days = Math.round((midnight(when) - midnight(now.getTime())) / 86_400_000);

  return days <= AUTO_PIN_DAYS && days >= -1;
}

function midnight(instant: number): number {
  return Math.floor(instant / 86_400_000) * 86_400_000;
}

export const ITEM_TYPES: { value: SetItemType; label: string }[] = [
  { value: 'announcement', label: 'Announcement' },
  { value: 'scripture', label: 'Scripture' },
  { value: 'prayer', label: 'Prayer' },
  { value: 'video', label: 'Video cue' },
  { value: 'blank', label: 'Blank / logo' },
  { value: 'text', label: 'Free text' },
];
