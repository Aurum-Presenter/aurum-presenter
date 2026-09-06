import { useLiveQuery } from 'dexie-react-hooks';
import { useEffect, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import { uuidv7 } from '../db/uuid';
import { importFile, type ImportResult } from '../library/importer';
import { SheetsRepository } from '../sheets/repository';

/**
 * Files that arrived from outside the app: opened from the file system with Aurum, or shared to
 * it from another app.
 *
 * Charts go through exactly the same parser as the import dialog — there is one importer, not a
 * second one for shared files. A PDF is a sheet, and a sheet needs to know which song it
 * belongs to, so it asks.
 */
const SHARE_CACHE = 'aurum-shared-files';

export function SharePage() {
  const { db, engine, library, sheets } = useWorkspace();
  const navigate = useNavigate();

  const [files, setFiles] = useState<File[] | null>(null);
  const [results, setResults] = useState<ImportResult[]>([]);
  const [pdfs, setPdfs] = useState<File[]>([]);
  const [song, setSong] = useState('');

  const songs = useLiveQuery(
    async () => (await db.songs.filter((row) => row.deleted_at === null).toArray())
      .sort((a, b) => a.title.localeCompare(b.title)),
    [db],
    [],
  );

  useEffect(() => {
    void (async () => {
      const collected = [...takeHandedFiles()];

      // Anything the service worker parked from an Android share.
      const cache = await caches.open(SHARE_CACHE).catch(() => null);

      if (cache !== null) {
        for (const request of await cache.keys()) {
          const response = await cache.match(request);

          if (response !== undefined) {
            const name = decodeURIComponent(new URL(request.url).pathname.split('/').pop() ?? 'shared');
            collected.push(new File([await response.blob()], name.replace(/^\d+-/, ''), {
              type: response.headers.get('content-type') ?? '',
            }));
          }

          await cache.delete(request);
        }
      }

      setFiles(collected);
    })();
  }, []);

  useEffect(() => {
    if (files === null) {
      return;
    }

    void (async () => {
      const imported: ImportResult[] = [];
      const documents: File[] = [];

      for (const file of files) {
        if (file.type === 'application/pdf' || file.name.toLowerCase().endsWith('.pdf')) {
          documents.push(file);
          continue;
        }

        const result = importFile(file.name, await file.text().catch(() => ''));
        imported.push(result);

        if (result.song !== null) {
          const songId = await library.createSong({
            title: result.song.title,
            artist: result.song.artist,
            original_key: result.song.original_key,
            tempo: result.song.tempo,
            time_signature: result.song.time_signature,
          });

          await engine.record('arrangements', uuidv7(), 'upsert', {
            song_id: songId,
            name: 'Default',
            body: result.song.body,
            default_key: result.song.original_key,
            is_default: 1,
            position: 0,
            source_notation: result.song.sourceNotation,
            source_text: result.song.sourceText,
          });
        }
      }

      setResults(imported);
      setPdfs(documents);
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [files]);

  const attach = async (): Promise<void> => {
    for (const file of pdfs) {
      const problem = SheetsRepository.problemWith(file);

      if (problem === null) {
        await sheets.attach(song, file, { key: null, part: 'lead', label: null, arrangementId: null });
      }
    }

    navigate(`/song/${song}`);
  };

  if (files === null) {
    return <p className="p-6 text-sm text-slate-500">Looking at what was shared…</p>;
  }

  if (files.length === 0) {
    return (
      <div className="p-6 text-sm">
        <p className="text-slate-500">Nothing was shared with Aurum.</p>
        <Link className="underline" to="/library">Go to the library</Link>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-2xl p-4">
      <h2 className="mb-3 text-2xl font-semibold">Shared with Aurum</h2>

      {results.length > 0 && (
        <section className="mb-6">
          <h3 className="mb-1 font-semibold">
            {results.filter((result) => result.error === null).length} song(s) imported
          </h3>
          <ul className="space-y-1 text-sm">
            {results.map((result) => (
              <li key={result.filename} className={result.error === null ? '' : 'text-amber-700 dark:text-amber-400'}>
                <span className="font-mono text-xs">{result.filename}</span>
                {result.error === null ? ` — ${result.song!.title}` : ` — ${result.error}`}
              </li>
            ))}
          </ul>
        </section>
      )}

      {pdfs.length > 0 && (
        <section>
          <h3 className="mb-1 font-semibold">{pdfs.length} PDF(s) to attach</h3>
          <p className="mb-2 text-sm text-slate-500">A sheet belongs to a song. Which one?</p>

          <select
            className="mb-3 w-full rounded border border-slate-300 px-2 py-2 dark:border-slate-700 dark:bg-slate-900"
            value={song}
            onChange={(event) => setSong(event.target.value)}
          >
            <option value="">Choose a song…</option>
            {songs.map((option) => <option key={option.id} value={option.id}>{option.title}</option>)}
          </select>

          <button
            className="rounded bg-slate-900 px-4 py-2 text-sm text-white disabled:opacity-40 dark:bg-slate-100 dark:text-slate-900"
            disabled={song === ''}
            onClick={() => void attach()}
          >
            Attach {pdfs.length} sheet(s)
          </button>
        </section>
      )}

      <Link className="mt-6 block text-sm underline" to="/library">Done</Link>
    </div>
  );
}

/**
 * Files handed over by the OS when Aurum is used to open them. The launch queue is consumed
 * once, at start-up, and parked here for this page to pick up.
 */
const handed: File[] = [];

export function acceptLaunchFiles(): void {
  const queue = (window as unknown as {
    launchQueue?: { setConsumer: (consumer: (params: { files: FileSystemFileHandle[] }) => void) => void };
  }).launchQueue;

  queue?.setConsumer(({ files }) => {
    void (async () => {
      for (const handle of files) {
        handed.push(await handle.getFile());
      }

      if (handed.length > 0 && ! window.location.pathname.startsWith('/share')) {
        window.location.assign('/share?from=file-handler');
      }
    })();
  });
}

function takeHandedFiles(): File[] {
  return handed.splice(0, handed.length);
}
