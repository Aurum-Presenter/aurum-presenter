import { useLiveQuery } from 'dexie-react-hooks';
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import { formatKey, parseKey, transposeKey, type Key } from '../chart/notes';
import type { Sheet } from '../db/schema';
import { SheetsRepository, WARN_SHEET_BYTES } from './repository';
import { explain, PARTS, selectSheet, type Part } from './selection';

/**
 * The sheets attached to a song: which one is being offered for the key on screen, why, and
 * everything needed to attach another.
 *
 * A sheet whose file this device has never downloaded says exactly that. It is a normal state —
 * the row synced and the file did not — and it is offered with the thing that fixes it.
 */
export function SheetsPanel({
  songId,
  songKey,
  part,
  onPart,
  pinned,
  onPinned,
}: {
  songId: string;
  songKey: Key | null;
  part: Part | null;
  onPart: (part: Part | null) => void;
  pinned: boolean;
  onPinned: (pinned: boolean) => void;
}) {
  const { db, canEdit, sheets: repository } = useWorkspace();
  const [attaching, setAttaching] = useState(false);

  const sheets = useLiveQuery(
    async () => (await db.sheets.where('song_id').equals(songId).filter((row) => row.deleted_at === null).toArray())
      .sort((a, b) => a.position - b.position),
    [db, songId],
    [],
  );

  const cached = useLiveQuery(async () => new Set((await db.blobs.toArray()).map((blob) => blob.sheet_id)), [db], new Set<string>());
  const chosen = selectSheet(sheets, songKey, part);
  const banner = chosen === null ? null : explain(chosen, songKey, part);

  return (
    <section className="mt-8">
      <div className="mb-2 flex flex-wrap items-center gap-3">
        <h3 className="font-semibold">Sheets</h3>

        <label className="flex items-center gap-1 text-sm text-slate-500">
          My part
          <select
            className="rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
            value={part ?? ''}
            onChange={(event) => onPart(event.target.value === '' ? null : event.target.value as Part)}
          >
            <option value="">any</option>
            {PARTS.map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </label>

        <label className="flex items-center gap-1 text-sm text-slate-500" title="Download this song's sheets and keep them">
          <input type="checkbox" checked={pinned} onChange={(event) => onPinned(event.target.checked)} />
          Keep offline
        </label>

        {canEdit && (
          <button className="ml-auto text-sm underline" onClick={() => setAttaching(true)}>Attach a sheet</button>
        )}
      </div>

      {sheets.length === 0 ? (
        <p className="text-sm text-slate-500">
          No sheets for this song. The chart above is always available; a PDF is optional.
        </p>
      ) : (
        <>
          {banner !== null && (
            <p className="mb-2 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900">{banner}</p>
          )}

          <ul className="divide-y divide-slate-200 dark:divide-slate-800">
            {sheets.map((sheet) => (
              <SheetRow
                key={sheet.id}
                sheet={sheet}
                songId={songId}
                chosen={chosen?.sheet.id === sheet.id}
                cached={cached.has(sheet.id)}
                canEdit={canEdit}
                repository={repository}
              />
            ))}
          </ul>
        </>
      )}

      {attaching && (
        <AttachDialog songId={songId} songKey={songKey} onClose={() => setAttaching(false)} repository={repository} />
      )}
    </section>
  );
}

function SheetRow(props: {
  sheet: Sheet;
  songId: string;
  chosen: boolean;
  cached: boolean;
  canEdit: boolean;
  repository: SheetsRepository;
}) {
  const { sheet } = props;
  const uploaded = sheet.sha256 !== null;

  return (
    <li className="flex flex-wrap items-baseline gap-2 py-2 text-sm">
      <Link className={props.chosen ? 'font-medium underline' : 'underline'} to={`/song/${props.songId}/sheet/${sheet.id}`}>
        {sheet.part ?? 'other'}{sheet.sheet_key === null ? '' : ` · ${sheet.sheet_key}`}
      </Link>

      {sheet.label !== null && <span className="text-slate-500">{sheet.label}</span>}
      {sheet.page_count !== null && (
        <span className="text-slate-400">{sheet.page_count} page{sheet.page_count === 1 ? '' : 's'}</span>
      )}
      {props.chosen && <span className="rounded-full bg-sky-100 px-2 text-xs text-sky-900">shown for this key</span>}

      {! uploaded && <span className="text-xs text-amber-700 dark:text-amber-400">waiting to upload</span>}
      {uploaded && ! props.cached && (
        <span className="text-xs text-slate-500" title="The row synced, the file has not been downloaded">
          not downloaded{sheet.size === null ? '' : ` · ${fileSize(sheet.size)}`}
        </span>
      )}

      {props.canEdit && (
        <span className="ml-auto flex gap-3">
          <label className="cursor-pointer underline">
            Replace
            <input
              type="file"
              className="hidden"
              accept=".pdf,image/png,image/jpeg"
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (file !== undefined) void props.repository.replace(sheet.id, file);
              }}
            />
          </label>
          <button className="underline text-red-700 dark:text-red-400" onClick={() => void props.repository.remove(sheet.id)}>
            Delete
          </button>
        </span>
      )}
    </li>
  );
}

function AttachDialog(props: {
  songId: string;
  songKey: Key | null;
  repository: SheetsRepository;
  onClose: () => void;
}) {
  const [file, setFile] = useState<File | null>(null);
  const [key, setKey] = useState(props.songKey === null ? '' : formatKey(props.songKey));
  const [part, setPart] = useState<Part>('lead');
  const [label, setLabel] = useState('');
  const [problem, setProblem] = useState<string | null>(null);

  const keys = Array.from({ length: 12 }, (_, semitones) => transposeKey(parseKey('C')!, semitones));

  const attach = async (): Promise<void> => {
    if (file === null) {
      setProblem('Choose a file first.');
      return;
    }

    const found = SheetsRepository.problemWith(file);

    if (found !== null) {
      setProblem(found);
      return;
    }

    await props.repository.attach(props.songId, file, {
      key: key === '' ? null : key,
      part,
      label: label.trim() === '' ? null : label.trim(),
      arrangementId: null,
    });

    props.onClose();
  };

  return (
    <div className="fixed inset-0 z-20 flex items-center justify-center bg-slate-900/50 p-6" onClick={props.onClose}>
      <div className="w-96 rounded bg-white p-4 shadow-lg dark:bg-slate-900" onClick={(event) => event.stopPropagation()}>
        <h2 className="mb-3 font-semibold">Attach a sheet</h2>

        <input
          type="file"
          accept=".pdf,image/png,image/jpeg"
          className="mb-3 block w-full text-sm"
          onChange={(event) => { setFile(event.target.files?.[0] ?? null); setProblem(null); }}
        />

        {file !== null && file.size > WARN_SHEET_BYTES && (
          <p className="mb-3 text-xs text-amber-700 dark:text-amber-400">
            {Math.round(file.size / 1024 / 1024)} MB will be kept on every device that pins this song.
          </p>
        )}

        <div className="mb-3 flex gap-2 text-sm">
          <label className="flex-1">
            Key
            <select className="w-full rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700" value={key} onChange={(event) => setKey(event.target.value)}>
              <option value="">any key</option>
              {keys.map((option) => <option key={formatKey(option)} value={formatKey(option)}>{formatKey(option)}</option>)}
            </select>
          </label>

          <label className="flex-1">
            Part
            <select className="w-full rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700" value={part} onChange={(event) => setPart(event.target.value as Part)}>
              {PARTS.map((option) => <option key={option} value={option}>{option}</option>)}
            </select>
          </label>
        </div>

        <input
          className="mb-3 w-full rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-950"
          placeholder="Label — SATB, Kate's copy…"
          value={label}
          onChange={(event) => setLabel(event.target.value)}
        />

        {problem !== null && <p className="mb-3 text-sm text-red-700">{problem}</p>}

        <div className="flex gap-2">
          <button className="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900" onClick={() => void attach()}>
            Attach
          </button>
          <button className="text-sm underline" onClick={props.onClose}>Cancel</button>
        </div>
      </div>
    </div>
  );
}

/** Sizes a musician can act on: a 40 MB score is a decision, 17 KB is not. */
function fileSize(bytes: number): string {
  return bytes < 1024 * 1024
    ? `${Math.max(1, Math.round(bytes / 1024))} KB`
    : `${Math.round(bytes / 1024 / 1024 * 10) / 10} MB`;
}
