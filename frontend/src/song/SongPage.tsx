import { useLiveQuery } from 'dexie-react-hooks';
import { useMemo, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import { ChartControls } from '../chart/ChartControls';
import { ChartEditor, type SaveInput } from '../chart/ChartEditor';
import { ChartView, type DisplayPrefs } from '../chart/ChartView';
import { effectiveKey, sourceKey } from '../chart/effectiveKey';
import { formatKey, parseKey, transposeKey, type Key } from '../chart/notes';
import type { Arrangement, Song } from '../db/schema';
import { uuidv7 } from '../db/uuid';
import { listOf } from '../library/repository';
import { useDisplay } from '../prefs/display';
import { NO_PREFS, readSongPrefs, writeSongPrefs, type SongPrefs } from '../prefs/songPrefs';
import { SongMetadataDrawer } from './SongMetadataDrawer';

/**
 * A song's chart: read it in your key, or edit it.
 *
 * Everything on this page comes out of IndexedDB, so it renders with the radio off. The only
 * thing the network does here is carry the change to everyone else, later.
 */
export function SongPage({ edit = false }: { edit?: boolean }) {
  const { db, engine, library, me, canEdit } = useWorkspace();
  const { songId } = useParams();
  const navigate = useNavigate();

  const [editing, setEditing] = useState(edit);
  const [drawer, setDrawer] = useState(false);
  const [respelled, setRespelled] = useState(false);
  const [prefsVersion, setPrefsVersion] = useState(0);
  const [display, setDisplay] = useDisplay();

  const song = useLiveQuery(() => db.songs.get(songId!), [db, songId]);

  const arrangements = useLiveQuery(
    async () => (await db.arrangements
      .where('song_id').equals(songId!)
      .filter((row) => row.deleted_at === null)
      .toArray())
      .sort((a, b) => a.position - b.position || a.name.localeCompare(b.name)),
    [db, songId],
    [],
  );

  const prefs = useLiveQuery(
    () => readSongPrefs(db, me.id, songId!),
    [db, me.id, songId, prefsVersion],
    NO_PREFS,
  );

  const arrangement = useMemo(() => {
    const chosen = arrangements.find((row) => row.id === prefs.arrangement_id);

    // One default per song is a client invariant, so a library that somehow holds two still
    // opens deterministically rather than picking whichever row Dexie returned first.
    return chosen ?? arrangements.find((row) => row.is_default === 1) ?? arrangements[0] ?? null;
  }, [arrangements, prefs.arrangement_id]);

  if (song === undefined) {
    return <p className="p-6 text-sm text-slate-500">Loading…</p>;
  }

  if (song === null || song.deleted_at !== null) {
    return (
      <div className="p-6 text-sm">
        <p className="text-slate-500">This song has been deleted.</p>
        <Link className="underline" to="/library/trash">Open Trash</Link>
      </div>
    );
  }

  const written = sourceKey({
    arrangementDefault: arrangement?.default_key ?? null,
    songOriginal: song.original_key,
  });

  const target = effectiveKey({
    preferred: prefs.preferred_key,
    arrangementDefault: arrangement?.default_key ?? null,
    songOriginal: song.original_key,
  });

  // The reader's own capo wins; with none chosen, the arranger's suggestion stands.
  const capo = prefs.capo ?? arrangement?.capo_hint ?? 0;

  const update = async (changes: Partial<SongPrefs>): Promise<void> => {
    await writeSongPrefs(db, engine, me.id, songId!, { ...prefs, ...changes });
    setPrefsVersion((value) => value + 1);
  };

  const createArrangement = async (): Promise<void> => {
    const id = uuidv7();

    await engine.record('arrangements', id, 'upsert', {
      song_id: song.id,
      name: arrangements.length === 0 ? 'Default' : `Arrangement ${arrangements.length + 1}`,
      body: '',
      is_default: arrangements.length === 0 ? 1 : 0,
      position: arrangements.length,
      source_notation: 'chordpro',
    });

    await update({ arrangement_id: id });
    setEditing(true);
  };

  const changeArrangement = async (id: string, changes: Record<string, unknown>): Promise<void> => {
    await engine.record('arrangements', id, 'upsert', changes);
  };

  /**
   * Exactly one arrangement per song is the default. The old default is cleared first, in its
   * own operation, so the outbox replays the pair in an order that never shows two defaults.
   */
  const makeDefault = async (id: string): Promise<void> => {
    for (const row of arrangements.filter((row) => row.is_default === 1 && row.id !== id)) {
      await engine.record('arrangements', row.id, 'upsert', { is_default: 0 });
    }

    await changeArrangement(id, { is_default: 1 });
  };

  const save = async (input: SaveInput): Promise<void> => {
    if (arrangement === null) {
      return;
    }

    await engine.record('arrangements', arrangement.id, 'upsert', {
      body: input.body,
      source_notation: input.sourceNotation,
      source_text: input.sourceText,
    });
  };

  const setSongKey = async (key: Key): Promise<void> => {
    await engine.record('songs', song.id, 'upsert', { original_key: formatKey(key) });
  };

  return (
    <div className="mx-auto max-w-5xl p-4">
      <Link className="text-sm underline" to="/library">← Library</Link>

      <div className="mb-3 mt-2 flex flex-wrap items-baseline gap-3">
        <h2 className="text-2xl font-semibold">{song.title}</h2>
        {song.artist !== null && <span className="text-slate-500">{song.artist}</span>}
        {song.tempo !== null && <span className="text-sm text-slate-500">{song.tempo} bpm</span>}
        {song.time_signature !== null && <span className="text-sm text-slate-500">{song.time_signature}</span>}
        {listOf(song.tags).map((tag) => (
          <span key={tag} className="rounded bg-slate-100 px-2 text-xs text-slate-600 dark:bg-slate-800 dark:text-slate-300">{tag}</span>
        ))}

        {canEdit && (
          <span className="ml-auto flex gap-3 text-sm">
            <button className="underline" onClick={() => setDrawer(true)}>Details</button>
            <button className="underline" onClick={() => void library.duplicateSong(song.id).then((id) => id !== null && navigate(`/song/${id}`))}>
              Duplicate
            </button>
            <button className="underline" onClick={() => void library.setArchived(song.id, song.archived === 0)}>
              {song.archived === 1 ? 'Unarchive' : 'Archive'}
            </button>
            <button
              className="underline text-red-700 dark:text-red-400"
              onClick={() => { void library.deleteSong(song.id); navigate('/library'); }}
            >
              Delete
            </button>
          </span>
        )}
      </div>

      {song.subtitle !== null && <p className="mb-3 text-slate-500">{song.subtitle}</p>}

      {written === null ? (
        <NoKeyYet canEdit={canEdit} onPick={(key) => void setSongKey(key)} />
      ) : (
        <ChartControls
          arrangements={arrangements.map((row) => ({ id: row.id, name: row.name }))}
          arrangementId={arrangement?.id ?? null}
          onArrangement={(id) => void update({ arrangement_id: id })}
          target={target.key}
          source={target.source}
          original={written}
          onKey={(key) => void update({ preferred_key: key === null ? null : formatKey(key) })}
          capo={capo}
          onCapo={(next) => void update({ capo: next })}
          display={display}
          onDisplay={setDisplay}
          respelled={respelled}
          canEdit={canEdit}
          editing={editing}
          onToggleEdit={() => setEditing((value) => ! value)}
        />
      )}

      <div className="mt-4">
        {arrangement === null ? (
          <EmptyChart canEdit={canEdit} onCreate={() => void createArrangement()} />
        ) : editing && canEdit ? (
          <>
            <ArrangementSettings
              arrangement={arrangement}
              onChange={(changes) => void changeArrangement(arrangement.id, changes)}
              onMakeDefault={() => void makeDefault(arrangement.id)}
            />
            <ChartEditor
              body={arrangement.body}
              onSave={(input) => void save(input)}
              preview={(body) => (
                <ChartView
                  cacheKey=""
                  body={body}
                  source={written ?? parseKey('C')!}
                  target={target.key ?? written ?? parseKey('C')!}
                  capo={capo}
                  display={display}
                />
              )}
            />
          </>
        ) : (
          <ChartBody
            arrangement={arrangement}
            written={written}
            target={target.key}
            capo={capo}
            display={display}
            onRespelled={setRespelled}
          />
        )}
      </div>

      {canEdit && arrangements.length > 0 && ! editing && (
        <button className="mt-6 text-sm underline" onClick={() => void createArrangement()}>
          Add another arrangement
        </button>
      )}

      {drawer && <SongMetadataDrawer song={song} onClose={() => setDrawer(false)} />}
    </div>
  );
}

/** Name, written key and suggested capo — the parts of an arrangement that are not the chart. */
function ArrangementSettings(props: {
  arrangement: Arrangement;
  onChange: (changes: Record<string, unknown>) => void;
  onMakeDefault: () => void;
}) {
  const keys = Array.from({ length: 12 }, (_, semitones) => transposeKey(parseKey('C')!, semitones));

  return (
    <div className="mb-3 flex flex-wrap items-center gap-2 text-sm">
      <input
        className="rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-900"
        value={props.arrangement.name}
        onChange={(event) => props.onChange({ name: event.target.value })}
        placeholder="Arrangement name"
      />

      <label className="flex items-center gap-1">
        Written in
        <select
          className="rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
          value={props.arrangement.default_key ?? ''}
          onChange={(event) => props.onChange({ default_key: event.target.value === '' ? null : event.target.value })}
        >
          <option value="">the song's key</option>
          {keys.map((key) => <option key={formatKey(key)} value={formatKey(key)}>{formatKey(key)}</option>)}
        </select>
      </label>

      <label className="flex items-center gap-1">
        Suggested capo
        <select
          className="rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
          value={props.arrangement.capo_hint ?? ''}
          onChange={(event) => props.onChange({ capo_hint: event.target.value === '' ? null : Number(event.target.value) })}
        >
          <option value="">none</option>
          {Array.from({ length: 12 }, (_, fret) => <option key={fret} value={fret}>{fret}</option>)}
        </select>
      </label>

      {props.arrangement.is_default === 1 ? (
        <span className="text-slate-500">Default arrangement</span>
      ) : (
        <button className="underline" onClick={props.onMakeDefault}>Make default</button>
      )}
    </div>
  );
}

function ChartBody(props: {
  arrangement: Arrangement;
  written: Key | null;
  target: Key | null;
  capo: number;
  display: DisplayPrefs;
  onRespelled: (value: boolean) => void;
}) {
  if (props.arrangement.body.trim() === '') {
    return <p className="text-sm text-slate-500">This arrangement has no chart yet.</p>;
  }

  const written = props.written ?? parseKey('C')!;

  return (
    <ChartView
      cacheKey={`${props.arrangement.id}:${props.arrangement.updated_at}`}
      body={props.arrangement.body}
      source={written}
      target={props.target ?? written}
      capo={props.capo}
      display={props.display}
      onRespelled={props.onRespelled}
    />
  );
}

function EmptyChart({ canEdit, onCreate }: { canEdit: boolean; onCreate: () => void }) {
  return (
    <div className="rounded border border-dashed border-slate-300 p-6 text-center dark:border-slate-700">
      <p className="text-sm text-slate-500">No chart yet.</p>
      {canEdit && (
        <button className="mt-3 rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900" onClick={onCreate}>
          Add a chart
        </button>
      )}
    </div>
  );
}

/**
 * Transposition needs to know what the chart is written in. Until the song says, there is no
 * interval to move by — so the page asks once rather than guessing and re-lettering wrongly.
 */
function NoKeyYet({ canEdit, onPick }: { canEdit: boolean; onPick: (key: Key) => void }) {
  const keys = Array.from({ length: 12 }, (_, semitones) => transposeKey(parseKey('C')!, semitones));

  if (! canEdit) {
    return <p className="text-sm text-slate-500">This song has no key set, so it cannot be transposed.</p>;
  }

  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 pb-3 text-sm dark:border-slate-800">
      <span className="text-slate-500">What key is this chart written in?</span>
      {keys.map((key) => (
        <button
          key={formatKey(key)}
          className="rounded border border-slate-200 px-2 py-1 dark:border-slate-700"
          onClick={() => onPick(key)}
        >
          {formatKey(key)}
        </button>
      ))}
    </div>
  );
}

export type { Song };
