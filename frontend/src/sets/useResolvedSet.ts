import { useLiveQuery } from 'dexie-react-hooks';
import { useWorkspace } from '../app/workspace';
import { effectiveKey, sourceKey, type KeySource } from '../chart/effectiveKey';
import type { Key } from '../chart/notes';
import type { Arrangement, SetItem, SetRecord, Song } from '../db/schema';
import type { SongPrefs } from '../prefs/songPrefs';
import { byRank } from './rank';

/**
 * A set, resolved into what each item actually shows: the song, the arrangement, and the key
 * this reader will see.
 *
 * The set's key override wins over the member's own preferred key (business rule 4) — that is
 * the whole point of a band deciding a key for the night — and everything below it stays
 * personal.
 */
export interface ResolvedItem {
  item: SetItem;
  song: Song | null;
  arrangement: Arrangement | null;
  /** The key the chart is written in, which transposition measures from. */
  written: Key | null;
  /** The key this reader sees. */
  key: Key | null;
  source: KeySource;
  capo: number;
  title: string;
  /** True when the item points at a song that has since been deleted. */
  missing: boolean;
}

export interface ResolvedSet {
  set: SetRecord | null;
  items: ResolvedItem[];
}

export function useResolvedSet(setId: string | undefined): ResolvedSet {
  const { db, me } = useWorkspace();

  const data = useLiveQuery(
    async () => {
      if (setId === undefined) {
        return null;
      }

      const set = (await db.sets.get(setId)) ?? null;

      const items = byRank(
        await db.set_items.where('set_id').equals(setId).filter((row) => row.deleted_at === null).toArray(),
      );

      const songs = new Map((await db.songs.toArray()).map((song) => [song.id, song]));
      const arrangements = await db.arrangements.filter((row) => row.deleted_at === null).toArray();

      // One read of this member's chart preferences, rather than one per item.
      const prefs = new Map<string, SongPrefs>();

      for (const row of await db.preferences.filter((p) => p.user_id === me.id && p.name === 'chart').toArray()) {
        if (row.scope_id !== null && row.value !== null && row.deleted_at === null) {
          try {
            prefs.set(row.scope_id, JSON.parse(row.value) as SongPrefs);
          } catch {
            // A preference row we cannot read is a preference the reader does not have.
          }
        }
      }

      return { set, items, songs, arrangements, prefs };
    },
    [db, setId, me.id],
    null,
  );

  if (data === null) {
    return { set: null, items: [] };
  }

  const items = data.items.map((item): ResolvedItem => {
    const song = item.song_id === null ? null : data.songs.get(item.song_id) ?? null;
    const alive = song !== null && song.deleted_at === null;
    const preference = item.song_id === null ? undefined : data.prefs.get(item.song_id);

    const forSong = data.arrangements.filter((row) => row.song_id === item.song_id);
    const arrangement = forSong.find((row) => row.id === item.arrangement_id)
      ?? forSong.find((row) => row.id === preference?.arrangement_id)
      ?? forSong.find((row) => row.is_default === 1)
      ?? forSong[0]
      ?? null;

    const written = sourceKey({
      arrangementDefault: arrangement?.default_key ?? null,
      songOriginal: song?.original_key ?? null,
    });

    const resolved = effectiveKey({
      setOverride: item.key_override,
      preferred: preference?.preferred_key ?? null,
      arrangementDefault: arrangement?.default_key ?? null,
      songOriginal: song?.original_key ?? null,
    });

    return {
      item,
      song: alive ? song : null,
      arrangement: alive ? arrangement : null,
      written,
      key: resolved.key,
      source: resolved.source,
      capo: item.capo_override ?? preference?.capo ?? arrangement?.capo_hint ?? 0,
      title: item.title_snapshot ?? song?.title ?? contentTitle(item),
      missing: item.song_id !== null && ! alive,
    };
  });

  return { set: data.set, items };
}

function contentTitle(item: SetItem): string {
  if (item.item_type === null) {
    return 'Untitled';
  }

  const first = (item.content ?? '').split('\n')[0]!.trim();

  return first === '' ? item.item_type : first;
}
