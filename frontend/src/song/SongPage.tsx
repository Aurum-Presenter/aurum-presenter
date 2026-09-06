import { useCallback, useEffect, useMemo, useState } from 'react';
import { ChartControls } from '../chart/ChartControls';
import { ChartEditor, type SaveInput } from '../chart/ChartEditor';
import { ChartView } from '../chart/ChartView';
import { effectiveKey, sourceKey } from '../chart/effectiveKey';
import { formatKey, parseKey, transposeKey, type Key } from '../chart/notes';
import type { Arrangement, Song, WorkspaceDb } from '../db/schema';
import { uuidv7 } from '../db/uuid';
import { loadDisplay, saveDisplay } from '../prefs/display';
import { NO_PREFS, readSongPrefs, writeSongPrefs, type SongPrefs } from '../prefs/songPrefs';
import type { SyncEngine } from '../sync/engine';

/**
 * A song's chart: read it in your key, or edit it.
 *
 * Everything on this page comes out of IndexedDB, so it renders with the radio off. The only
 * thing the network does here is carry the change to everyone else, later.
 */

export interface SongPageProps {
  db: WorkspaceDb;
  engine: SyncEngine;
  song: Song;
  userId: string;
  canEdit: boolean;
  /** A set can override the key for the duration of that set — business rule 7, first level. */
  setKeyOverride?: string | null;
  onBack: () => void;
  onChanged: () => void;
}

export function SongPage(props: SongPageProps) {
  const { db, engine, song, userId } = props;

  const [arrangements, setArrangements] = useState<Arrangement[]>([]);
  const [prefs, setPrefs] = useState<SongPrefs>(NO_PREFS);
  const [display, setDisplay] = useState(loadDisplay);
  const [editing, setEditing] = useState(false);
  const [respelled, setRespelled] = useState(false);

  const load = useCallback(async () => {
    const rows = await db.arrangements
      .where('song_id').equals(song.id)
      .filter((row) => row.deleted_at === null)
      .toArray();

    setArrangements(rows.sort((a, b) => a.position - b.position || a.name.localeCompare(b.name)));
    setPrefs(await readSongPrefs(db, userId, song.id));
  }, [db, song.id, userId]);

  useEffect(() => { void load(); }, [load]);

  const arrangement = useMemo(() => {
    const chosen = arrangements.find((row) => row.id === prefs.arrangement_id);

    // One default per song is a client invariant, so a library that somehow holds two still
    // opens deterministically rather than picking whichever row Dexie returned first.
    return chosen ?? arrangements.find((row) => row.is_default === 1) ?? arrangements[0] ?? null;
  }, [arrangements, prefs.arrangement_id]);

  const written = sourceKey({
    arrangementDefault: arrangement?.default_key ?? null,
    songOriginal: song.original_key,
  });

  // The reader's own capo wins; with none chosen, the arranger's suggestion stands.
  const capo = prefs.capo ?? arrangement?.capo_hint ?? 0;

  const target = effectiveKey({
    setOverride: props.setKeyOverride ?? null,
    preferred: prefs.preferred_key,
    arrangementDefault: arrangement?.default_key ?? null,
    songOriginal: song.original_key,
  });

  const update = async (changes: Partial<SongPrefs>): Promise<void> => {
    const next = { ...prefs, ...changes };
    setPrefs(next);
    await writeSongPrefs(db, engine, userId, song.id, next);
    props.onChanged();
  };

  const setDisplayPrefs = (next: typeof display): void => {
    setDisplay(next);
    saveDisplay(next);
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
    await load();
    setEditing(true);
  };

  const changeArrangement = async (id: string, changes: Record<string, unknown>): Promise<void> => {
    await engine.record('arrangements', id, 'upsert', changes);
    await load();
    props.onChanged();
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

    await load();
    props.onChanged();
  };

  const setSongKey = async (key: Key): Promise<void> => {
    await engine.record('songs', song.id, 'upsert', { original_key: formatKey(key) });
    props.onChanged();
  };

  return (
    <div className="mx-auto max-w-5xl p-4">
      <button className="mb-3 text-sm underline" onClick={props.onBack}>← Library</button>

      <h2 className="text-2xl font-semibold">{song.title}</h2>
      {song.subtitle !== null && <p className="mb-3 text-slate-500">{song.subtitle}</p>}

      {written === null ? (
        <NoKeyYet canEdit={props.canEdit} onPick={setSongKey} />
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
          onDisplay={setDisplayPrefs}
          respelled={respelled}
          canEdit={props.canEdit}
          editing={editing}
          onToggleEdit={() => setEditing((value) => ! value)}
        />
      )}

      <div className="mt-4">
        {arrangement === null ? (
          <EmptyChart canEdit={props.canEdit} onCreate={() => void createArrangement()} />
        ) : editing && props.canEdit ? (
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

      {props.canEdit && arrangements.length > 0 && ! editing && (
        <button className="mt-6 text-sm underline" onClick={() => void createArrangement()}>
          Add another arrangement
        </button>
      )}
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
  display: ReturnType<typeof loadDisplay>;
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
