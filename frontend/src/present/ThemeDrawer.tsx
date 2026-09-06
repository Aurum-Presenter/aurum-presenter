import { useLiveQuery } from 'dexie-react-hooks';
import { useWorkspace } from '../app/workspace';
import { useState } from 'react';
import { uuidv7 } from '../db/uuid';
import { uploadBackground } from './background';
import type { Theme } from './session';

/**
 * The theme editor.
 *
 * A theme belongs to the workspace and syncs like any other record, so a band's screens agree
 * without anyone configuring a second laptop. A change made here applies to the running session
 * immediately; saving it to the workspace is a separate, deliberate act.
 */
export function ThemeDrawer({
  theme,
  onChange,
  onClose,
}: {
  theme: Theme;
  onChange: (theme: Theme) => void;
  onClose: () => void;
}) {
  const { db, engine, workspace, canEdit } = useWorkspace();
  const [uploading, setUploading] = useState<string | null>(null);

  const saved = useLiveQuery(
    () => db.presenter_themes.filter((row) => row.deleted_at === null).toArray(),
    [db],
    [],
  );

  const set = <K extends keyof Theme>(field: K, value: Theme[K]): void => onChange({ ...theme, [field]: value });

  const saveToWorkspace = async (): Promise<void> => {
    const id = saved.find((row) => row.name === theme.name)?.id ?? uuidv7();

    await engine.record('presenter_themes', id, 'upsert', {
      name: theme.name,
      is_default: saved.length === 0 ? 1 : 0,
      font_family: theme.font_family,
      font_size_vh: theme.font_size_vh,
      text_color: theme.text_color,
      background_kind: theme.background_kind,
      background_value: theme.background_value,
      align: theme.align,
      safe_area_pct: theme.safe_area_pct,
      show_section_labels: theme.show_section_labels ? 1 : 0,
    });
  };

  return (
    <div className="fixed inset-0 z-20 flex justify-end bg-slate-900/40" onClick={onClose}>
      <div className="h-full w-80 overflow-auto bg-white p-4 text-sm shadow-xl dark:bg-slate-900" onClick={(event) => event.stopPropagation()}>
        <h2 className="mb-3 text-lg font-semibold">Theme</h2>

        {saved.length > 0 && (
          <label className="mb-3 block">
            <span className="text-slate-500">Workspace themes</span>
            <select
              className="w-full rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
              value={theme.id}
              onChange={(event) => {
                const found = saved.find((row) => row.id === event.target.value);

                if (found !== undefined) {
                  onChange({
                    id: found.id,
                    name: found.name,
                    font_family: found.font_family,
                    font_size_vh: found.font_size_vh,
                    text_color: found.text_color,
                    background_kind: found.background_kind,
                    background_value: found.background_value,
                    align: found.align,
                    safe_area_pct: found.safe_area_pct,
                    show_section_labels: found.show_section_labels === 1,
                  });
                }
              }}
            >
              <option value={theme.id}>{theme.name}</option>
              {saved.map((row) => <option key={row.id} value={row.id}>{row.name}</option>)}
            </select>
          </label>
        )}

        <Field label="Name">
          <input className="w-full rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-950" value={theme.name} onChange={(event) => set('name', event.target.value)} />
        </Field>

        <Field label="Text colour">
          <input type="color" value={theme.text_color} onChange={(event) => set('text_color', event.target.value)} />
        </Field>

        <Field label="Background">
          <input
            type="color"
            value={theme.background_kind === 'color' ? theme.background_value : '#000000'}
            onChange={(event) => onChange({ ...theme, background_kind: 'color', background_value: event.target.value })}
          />
        </Field>

        <Field label="Background image" hint="Optional; the colour shows through if it is not on this device">
          <input
            type="file"
            accept="image/png,image/jpeg,image/webp,image/avif"
            className="w-full text-xs"
            onChange={(event) => {
              const file = event.target.files?.[0];

              if (file === undefined) {
                return;
              }

              setUploading('Uploading…');
              void uploadBackground(db, workspace.id, file)
                .then((asset) => {
                  onChange({ ...theme, background_kind: 'image', background_value: asset.sha256 });
                  setUploading(null);
                })
                .catch((error: Error) => setUploading(error.message));
            }}
          />
          {uploading !== null && <p className="text-xs text-slate-500">{uploading}</p>}
          {theme.background_kind === 'image' && (
            <button
              className="mt-1 text-xs underline"
              onClick={() => onChange({ ...theme, background_kind: 'color', background_value: '#000000' })}
            >
              Remove the image
            </button>
          )}
        </Field>

        <Field label={`Maximum size — ${theme.font_size_vh}vh`}>
          <input type="range" min={4} max={16} step={0.5} value={theme.font_size_vh} onChange={(event) => set('font_size_vh', Number(event.target.value))} />
        </Field>

        <Field label={`Safe margin — ${theme.safe_area_pct}%`}>
          <input type="range" min={0} max={15} step={1} value={theme.safe_area_pct} onChange={(event) => set('safe_area_pct', Number(event.target.value))} />
        </Field>

        <Field label="Alignment">
          <select className="w-full rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700" value={theme.align} onChange={(event) => set('align', event.target.value as Theme['align'])}>
            <option value="center">Centre</option>
            <option value="left">Left</option>
          </select>
        </Field>

        <label className="mb-3 flex items-center gap-2">
          <input type="checkbox" checked={theme.show_section_labels} onChange={(event) => set('show_section_labels', event.target.checked)} />
          Show section labels on the audience screen
        </label>

        <p className="mb-3 text-xs text-slate-500">
          Changing the size rebuilds the slides at the next session; the running session keeps the
          slides it started with, so nothing moves under the operator mid-service.
        </p>

        <div className="flex gap-2">
          {canEdit && (
            <button className="rounded bg-slate-900 px-3 py-2 text-white dark:bg-slate-100 dark:text-slate-900" onClick={() => void saveToWorkspace()}>
              Save to the workspace
            </button>
          )}
          <button className="underline" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="mb-3 block">
      <span className="text-slate-500">{label}</span>
      {hint !== undefined && <span className="ml-2 text-xs text-slate-400">{hint}</span>}
      <div>{children}</div>
    </label>
  );
}
