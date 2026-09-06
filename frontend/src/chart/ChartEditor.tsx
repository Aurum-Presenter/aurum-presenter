import { useEffect, useMemo, useRef, useState } from 'react';
import { parseChart } from './chordpro';
import { detectNotation, toChordPro } from './overLyrics';

/**
 * The chart editor: plain ChordPro text on the left, live preview on the right, and a gutter
 * that names tokens it could not read.
 *
 * The gutter never blocks a save. A musician typing a chart ten minutes before a service is not
 * going to be stopped by a validator that thinks `[Hmm]` is a mistake.
 */

export interface SaveInput {
  body: string;
  sourceNotation: 'chordpro' | 'over_lyrics';
  /** The pre-conversion paste, kept for exactly one undo (business rule 1). */
  sourceText: string | null;
}

export interface ChartEditorProps {
  body: string;
  onSave: (input: SaveInput) => void;
  preview: (body: string) => React.ReactNode;
}

const AUTOSAVE_MS = 800;

export function ChartEditor({ body, onSave, preview }: ChartEditorProps) {
  const [draft, setDraft] = useState(body);
  const [converted, setConverted] = useState<{ from: string } | null>(null);
  const [asking, setAsking] = useState<string | null>(null);
  const [saved, setSaved] = useState(true);

  const notation = useRef<SaveInput['sourceNotation']>('chordpro');
  const sourceText = useRef<string | null>(null);
  const latest = useRef(draft);
  latest.current = draft;

  const warnings = useMemo(() => parseChart(draft).warnings, [draft]);

  const commit = (): void => {
    onSave({ body: latest.current, sourceNotation: notation.current, sourceText: sourceText.current });
    setSaved(true);
  };

  useEffect(() => {
    if (draft === body) {
      return;
    }

    setSaved(false);
    const timer = setTimeout(commit, AUTOSAVE_MS);

    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft]);

  const acceptPaste = (text: string, at: HTMLTextAreaElement): void => {
    const before = at.value.slice(0, at.selectionStart);
    const after = at.value.slice(at.selectionEnd);

    notation.current = 'over_lyrics';
    sourceText.current = text;
    setDraft(before + toChordPro(text) + after);
    setConverted({ from: before + text + after });
  };

  const onPaste = (event: React.ClipboardEvent<HTMLTextAreaElement>): void => {
    const text = event.clipboardData.getData('text/plain');

    if (text.trim() === '') {
      return;
    }

    const kind = detectNotation(text);

    if (kind === 'chordpro') {
      return;
    }

    event.preventDefault();

    if (kind === 'ambiguous') {
      // Business rule 2 can only decide when there is a chord line to see. With none, the user
      // knows what they pasted and we do not.
      setAsking(text);
      return;
    }

    acceptPaste(text, event.currentTarget);
  };

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <div>
        {converted !== null && (
          <div className="mb-2 flex items-center gap-3 rounded border border-sky-300 bg-sky-50 px-3 py-2 text-sm text-sky-900">
            <span>Converted from chords over lyrics.</span>
            <button
              className="underline"
              onClick={() => { setDraft(converted.from); notation.current = 'chordpro'; sourceText.current = null; setConverted(null); }}
            >
              Undo
            </button>
          </div>
        )}

        <textarea
          className="h-[60vh] w-full rounded border border-slate-300 bg-white p-3 font-mono text-sm leading-relaxed dark:border-slate-700 dark:bg-slate-900"
          spellCheck={false}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onPaste={onPaste}
          onBlur={() => { if (! saved) commit(); }}
          placeholder={'{title: Song}\n{verse: 1}\n[G]Type or paste a chart…'}
        />

        <div className="mt-2 flex items-start gap-4 text-xs text-slate-500">
          <span>{saved ? 'Saved' : 'Saving…'}</span>

          {warnings.length > 0 && (
            <ul className="space-y-0.5">
              {warnings.slice(0, 8).map((warning, index) => (
                <li key={index} className="text-amber-700 dark:text-amber-400">
                  Line {warning.line}: “{warning.token}” is not a chord — it will be shown as written.
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      <div className="rounded border border-slate-200 p-3 dark:border-slate-800">{preview(draft)}</div>

      {asking !== null && (
        <NotationPrompt
          onChordPro={() => { setDraft((current) => current + asking); setAsking(null); }}
          onOverLyrics={() => {
            notation.current = 'over_lyrics';
            sourceText.current = asking;
            setDraft((current) => current + toChordPro(asking));
            setAsking(null);
          }}
          onCancel={() => setAsking(null)}
        />
      )}
    </div>
  );
}

function NotationPrompt(props: { onChordPro: () => void; onOverLyrics: () => void; onCancel: () => void }) {
  return (
    <div className="fixed inset-0 z-10 flex items-center justify-center bg-slate-900/50 p-6">
      <div className="w-96 rounded bg-white p-4 shadow-lg dark:bg-slate-900">
        <h2 className="mb-2 font-semibold">Which notation is this?</h2>
        <p className="mb-4 text-sm text-slate-500">
          No chord line was recognised, so the format cannot be told from the text alone.
        </p>
        <div className="flex gap-2">
          <button className="rounded bg-slate-900 px-3 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900" onClick={props.onOverLyrics}>
            Chords over lyrics
          </button>
          <button className="rounded border border-slate-300 px-3 py-2 text-sm dark:border-slate-700" onClick={props.onChordPro}>
            ChordPro
          </button>
          <button className="ml-auto text-sm underline" onClick={props.onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
