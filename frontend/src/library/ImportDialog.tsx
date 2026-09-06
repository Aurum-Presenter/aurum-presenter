import { useState } from 'react';
import { useWorkspace } from '../app/workspace';
import { uuidv7 } from '../db/uuid';
import { importFile, type ImportResult } from './importer';

/**
 * Import of ChordPro and plain text files.
 *
 * Files are processed in chunks so a folder of two hundred charts cannot freeze the tab, and a
 * file that fails is reported by name and reason while every other file still lands.
 */
const CHUNK = 20;

export function ImportDialog({ folderId, onClose }: { folderId: string | null; onClose: () => void }) {
  const { library, engine } = useWorkspace();
  const [results, setResults] = useState<ImportResult[] | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async (files: FileList): Promise<void> => {
    setBusy(true);
    const collected: ImportResult[] = [];

    for (let start = 0; start < files.length; start += CHUNK) {
      const chunk = [...files].slice(start, start + CHUNK);

      for (const file of chunk) {
        const result = importFile(file.name, await file.text().catch(() => ''));
        collected.push(result);

        if (result.song !== null) {
          const songId = await library.createSong({
            title: result.song.title,
            folder_id: folderId,
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

      // Yield between chunks so the list of results paints as the import runs.
      setResults([...collected]);
      await new Promise((resolve) => setTimeout(resolve, 0));
    }

    setBusy(false);
  };

  const failures = (results ?? []).filter((result) => result.error !== null);
  const imported = (results ?? []).length - failures.length;

  return (
    <div className="fixed inset-0 z-20 flex items-center justify-center bg-slate-900/50 p-6">
      <div className="max-h-[80vh] w-[32rem] overflow-auto rounded bg-white p-4 shadow-lg dark:bg-slate-900">
        <h2 className="mb-2 font-semibold">Import songs</h2>
        <p className="mb-3 text-sm text-slate-500">
          ChordPro (<code>.cho</code>, <code>.chopro</code>, <code>.pro</code>) or plain
          chords-over-lyrics text. Files that cannot be read are listed; the rest still import.
        </p>

        <input
          type="file"
          multiple
          accept=".cho,.chopro,.chordpro,.crd,.pro,.txt,text/plain"
          className="mb-3 block w-full text-sm"
          onChange={(event) => { if (event.target.files !== null) void run(event.target.files); }}
        />

        {results !== null && (
          <div className="mb-3 text-sm">
            <p className="font-medium">
              {imported} imported{failures.length > 0 ? `, ${failures.length} could not be read` : ''}
              {busy ? ' …' : ''}
            </p>

            {failures.length > 0 && (
              <ul className="mt-2 space-y-1">
                {failures.map((failure) => (
                  <li key={failure.filename} className="text-amber-700 dark:text-amber-400">
                    <span className="font-mono text-xs">{failure.filename}</span> — {failure.error}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        <button className="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900" onClick={onClose}>
          {results === null ? 'Cancel' : 'Done'}
        </button>
      </div>
    </div>
  );
}
