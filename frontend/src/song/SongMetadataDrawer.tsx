import { useState } from 'react';
import { useWorkspace } from '../app/workspace';
import type { Song } from '../db/schema';
import { formatDuration, listOf, parseDuration, validateSong, type SongInput } from '../library/repository';

/**
 * The song's metadata, as a drawer over the chart.
 *
 * Validation is immediate and advisory: it names what is wrong, and refuses only the things
 * that would make the record meaningless — an empty title, a tempo no instrument plays.
 */
export function SongMetadataDrawer({ song, onClose }: { song: Song; onClose: () => void }) {
  const { library } = useWorkspace();

  const [form, setForm] = useState<SongInput & { duration: string; tagText: string; altText: string }>({
    title: song.title,
    subtitle: song.subtitle,
    artist: song.artist,
    authors: song.authors,
    ccli_number: song.ccli_number,
    copyright: song.copyright,
    notes: song.notes,
    original_key: song.original_key,
    tempo: song.tempo,
    time_signature: song.time_signature,
    folder_id: song.folder_id,
    duration: formatDuration(song.duration_sec),
    tagText: listOf(song.tags).join(', '),
    altText: listOf(song.alt_titles).join(', '),
  });

  const [problems, setProblems] = useState<string[]>([]);

  const set = <K extends keyof typeof form>(field: K, value: (typeof form)[K]): void =>
    setForm((current) => ({ ...current, [field]: value }));

  const submit = async (event: React.FormEvent): Promise<void> => {
    event.preventDefault();

    const input: SongInput = {
      ...form,
      tags: split(form.tagText),
      alt_titles: split(form.altText),
      duration_sec: form.duration.trim() === '' ? null : parseDuration(form.duration),
    };

    const found = validateSong(input);

    if (form.duration.trim() !== '' && input.duration_sec === null) {
      found.push('Length is mm:ss, for example 4:05.');
    }

    if (found.length > 0) {
      setProblems(found);
      return;
    }

    await library.updateSong(song.id, input);
    onClose();
  };

  return (
    <div className="fixed inset-0 z-20 flex justify-end bg-slate-900/40" onClick={onClose}>
      <form
        className="h-full w-full max-w-md overflow-auto bg-white p-4 shadow-xl dark:bg-slate-900"
        onClick={(event) => event.stopPropagation()}
        onSubmit={(event) => void submit(event)}
      >
        <h2 className="mb-3 text-lg font-semibold">Song details</h2>

        {problems.length > 0 && (
          <ul className="mb-3 rounded border border-red-300 bg-red-50 p-2 text-sm text-red-800">
            {problems.map((problem) => <li key={problem}>{problem}</li>)}
          </ul>
        )}

        <Field label="Title"><Text value={form.title} onChange={(value) => set('title', value)} /></Field>
        <Field label="Also known as" hint="Comma separated">
          <Text value={form.altText} onChange={(value) => set('altText', value)} />
        </Field>
        <Field label="Subtitle"><Text value={form.subtitle ?? ''} onChange={(value) => set('subtitle', value)} /></Field>
        <Field label="Artist"><Text value={form.artist ?? ''} onChange={(value) => set('artist', value)} /></Field>
        <Field label="Author"><Text value={form.authors ?? ''} onChange={(value) => set('authors', value)} /></Field>
        <Field label="Original key"><Text value={form.original_key ?? ''} onChange={(value) => set('original_key', value)} /></Field>

        <div className="grid grid-cols-3 gap-2">
          <Field label="Tempo">
            <Text
              value={form.tempo === null || form.tempo === undefined ? '' : String(form.tempo)}
              onChange={(value) => set('tempo', value.trim() === '' ? null : Number(value))}
            />
          </Field>
          <Field label="Time"><Text value={form.time_signature ?? ''} onChange={(value) => set('time_signature', value)} /></Field>
          <Field label="Length" hint="mm:ss"><Text value={form.duration} onChange={(value) => set('duration', value)} /></Field>
        </div>

        <Field label="Tags" hint="Comma separated"><Text value={form.tagText} onChange={(value) => set('tagText', value)} /></Field>
        <Field label="CCLI"><Text value={form.ccli_number ?? ''} onChange={(value) => set('ccli_number', value)} /></Field>
        <Field label="Copyright"><Text value={form.copyright ?? ''} onChange={(value) => set('copyright', value)} /></Field>

        <Field label="Notes" hint="Never shown on the audience screen">
          <textarea
            className="w-full rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-950"
            rows={3}
            value={form.notes ?? ''}
            onChange={(event) => set('notes', event.target.value)}
          />
        </Field>

        <div className="mt-4 flex gap-2">
          <button className="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900">Save</button>
          <button type="button" className="text-sm underline" onClick={onClose}>Cancel</button>
        </div>
      </form>
    </div>
  );
}

function split(text: string): string[] {
  return text.split(',').map((part) => part.trim()).filter((part) => part !== '');
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="mb-2 block text-sm">
      <span className="text-slate-500">{label}</span>
      {hint !== undefined && <span className="ml-2 text-xs text-slate-400">{hint}</span>}
      {children}
    </label>
  );
}

function Text({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  return (
    <input
      className="w-full rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-950"
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}
