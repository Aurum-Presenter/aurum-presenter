import { useLiveQuery } from 'dexie-react-hooks';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import { AnnotationLayer, type Stroke } from './AnnotationLayer';

/**
 * The sheet viewer.
 *
 * Rendering happens locally from the cached file — there is no streaming from the server and no
 * page-by-page fetch — because the whole point of pinning a set is that the PDF opens on a
 * stage with no signal. A file that is not here says so, plainly, with the size it would take.
 */
export function SheetViewerPage() {
  const { songId, sheetId } = useParams();
  const { db, blobs, engine, me, canEdit, online } = useWorkspace();

  const [file, setFile] = useState<Blob | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [pages, setPages] = useState(1);
  const [zoom, setZoom] = useState(1);
  const [fit, setFit] = useState<'width' | 'page' | 'free'>('width');
  const [rotation, setRotation] = useState(0);
  const [spread, setSpread] = useState(false);
  const [scope, setScope] = useState<'personal' | 'shared'>('personal');
  const [drawing, setDrawing] = useState(false);

  const sheet = useLiveQuery(() => db.sheets.get(sheetId!), [db, sheetId]);
  const frame = useRef<HTMLDivElement>(null);

  const annotations = useLiveQuery(
    async () => (await db.annotations.where('sheet_id').equals(sheetId!).toArray())
      .filter((row) => row.deleted_at === null && (row.scope === 'shared' || row.author_id === me.id)),
    [db, sheetId, me.id],
    [],
  );

  useEffect(() => {
    let cancelled = false;

    setFile(null);
    setFailure(null);

    blobs.fetchNow(sheetId!)
      .then((bytes) => { if (! cancelled) setFile(bytes); })
      .catch((error: Error) => { if (! cancelled) setFailure(error.message); });

    return () => { cancelled = true; };
  }, [blobs, sheetId, online]);

  useEffect(() => {
    const onResize = (): void => setSpread(window.innerWidth >= 900 && fit !== 'width');
    onResize();
    window.addEventListener('resize', onResize);

    return () => window.removeEventListener('resize', onResize);
  }, [fit]);

  useEffect(() => {
    const key = (event: KeyboardEvent): void => {
      if (event.key === 'ArrowRight' || event.key === 'PageDown') setPage((value) => Math.min(value + (spread ? 2 : 1), pages));
      if (event.key === 'ArrowLeft' || event.key === 'PageUp') setPage((value) => Math.max(value - (spread ? 2 : 1), 1));
    };

    window.addEventListener('keydown', key);

    return () => window.removeEventListener('keydown', key);
  }, [pages, spread]);

  const saveStrokes = useCallback(async (pageNumber: number, strokes: Stroke[]): Promise<void> => {
    const existing = (await db.annotations.where('sheet_id').equals(sheetId!).toArray())
      .find((row) => row.page === pageNumber && row.scope === scope && row.author_id === me.id);

    const id = existing?.id ?? crypto.randomUUID();

    await engine.record('annotations', id, 'upsert', {
      sheet_id: sheetId,
      page: pageNumber,
      scope,
      author_id: me.id,
      strokes: JSON.stringify(strokes),
    });
  }, [db, engine, me.id, scope, sheetId]);

  const strokesFor = (pageNumber: number): { strokes: Stroke[]; mine: Stroke[] } => {
    const all: Stroke[] = [];
    let mine: Stroke[] = [];

    for (const row of annotations) {
      if (row.page !== pageNumber) {
        continue;
      }

      let parsed: Stroke[] = [];

      try {
        parsed = JSON.parse(row.strokes) as Stroke[];
      } catch {
        parsed = [];
      }

      all.push(...parsed);

      if (row.author_id === me.id && row.scope === scope) {
        mine = parsed;
      }
    }

    return { strokes: all, mine };
  };

  if (sheet === undefined) {
    return <p className="p-6 text-sm text-slate-500">Loading…</p>;
  }

  return (
    <div className="mx-auto max-w-5xl p-4">
      <div className="mb-3 flex flex-wrap items-center gap-3 text-sm">
        <Link className="underline" to={`/song/${songId}`}>← Song</Link>
        <span className="font-medium">{sheet?.part ?? 'sheet'}{sheet?.sheet_key === null ? '' : ` · ${sheet?.sheet_key}`}</span>
        {sheet?.filename !== null && <span className="text-slate-500">{sheet?.filename}</span>}

        <span className="ml-auto flex flex-wrap items-center gap-2">
          <button className="rounded border border-slate-300 px-2 dark:border-slate-700" onClick={() => setPage((value) => Math.max(1, value - (spread ? 2 : 1)))}>←</button>
          <span>{page}{spread && page < pages ? `–${page + 1}` : ''} / {pages}</span>
          <button className="rounded border border-slate-300 px-2 dark:border-slate-700" onClick={() => setPage((value) => Math.min(pages, value + (spread ? 2 : 1)))}>→</button>

          <button className="rounded border border-slate-300 px-2 dark:border-slate-700" onClick={() => { setFit('free'); setZoom((value) => Math.max(0.25, value - 0.25)); }}>−</button>
          <button className="rounded border border-slate-300 px-2 dark:border-slate-700" onClick={() => { setFit('free'); setZoom((value) => Math.min(6, value + 0.25)); }}>+</button>
          <button className={`rounded border px-2 ${fit === 'width' ? 'border-sky-500' : 'border-slate-300 dark:border-slate-700'}`} onClick={() => setFit('width')}>fit width</button>
          <button className={`rounded border px-2 ${fit === 'page' ? 'border-sky-500' : 'border-slate-300 dark:border-slate-700'}`} onClick={() => setFit('page')}>fit page</button>
          <button className="rounded border border-slate-300 px-2 dark:border-slate-700" onClick={() => setRotation((value) => (value + 90) % 360)}>rotate</button>

          <button
            className={`rounded border px-2 ${drawing ? 'border-sky-500' : 'border-slate-300 dark:border-slate-700'}`}
            onClick={() => setDrawing((value) => ! value)}
          >
            {drawing ? 'done' : 'annotate'}
          </button>

          {drawing && (
            <select
              className="rounded border border-slate-300 bg-transparent px-1 dark:border-slate-700"
              value={scope}
              onChange={(event) => setScope(event.target.value as 'personal' | 'shared')}
            >
              <option value="personal">just me</option>
              {canEdit && <option value="shared">the whole band</option>}
            </select>
          )}
        </span>
      </div>

      <div ref={frame} className="rounded border border-slate-200 p-2 dark:border-slate-800">
        {failure !== null || (file === null && ! online) ? (
          <NotDownloaded sheet={sheet ?? null} reason={failure} />
        ) : file === null ? (
          <p className="py-16 text-center text-sm text-slate-500">Opening…</p>
        ) : (
          <PageStack
            file={file}
            mime={sheet?.mime_type ?? 'application/pdf'}
            page={page}
            spread={spread}
            zoom={zoom}
            fit={fit}
            rotation={rotation}
            container={frame}
            onPages={setPages}
            onFailure={setFailure}
            renderOverlay={(pageNumber, width, height) => {
              const { strokes, mine } = strokesFor(pageNumber);

              return (
                <AnnotationLayer
                  width={width}
                  height={height}
                  strokes={strokes}
                  mine={mine}
                  drawing={drawing}
                  onChange={(next) => void saveStrokes(pageNumber, next)}
                />
              );
            }}
          />
        )}
      </div>
    </div>
  );
}

function NotDownloaded({ sheet, reason }: { sheet: { size: number | null } | null; reason: string | null }) {
  const size = sheet?.size == null
    ? null
    : sheet.size < 1024 * 1024
      ? `${Math.max(1, Math.round(sheet.size / 1024))} KB`
      : `${Math.round(sheet.size / 1024 / 1024 * 10) / 10} MB`;

  return (
    <div className="py-16 text-center">
      <p className="text-sm text-slate-500">
        This sheet has not been downloaded to this device{size === null ? '' : ` (${size})`}.
      </p>
      <p className="mt-2 text-sm text-slate-500">
        Pin the song, or the set it is in, and it will be here the next time you have a connection.
      </p>
      {reason !== null && <p className="mt-2 text-xs text-slate-400">{reason}</p>}
    </div>
  );
}

/**
 * One or two pages, rendered from the local file.
 *
 * pdf.js is loaded on demand rather than in the main bundle: most sessions never open a sheet,
 * and the library is bigger than the rest of the app put together.
 */
function PageStack(props: {
  file: Blob;
  mime: string;
  page: number;
  spread: boolean;
  zoom: number;
  fit: 'width' | 'page' | 'free';
  rotation: number;
  container: React.RefObject<HTMLDivElement | null>;
  onPages: (pages: number) => void;
  onFailure: (reason: string) => void;
  renderOverlay: (page: number, width: number, height: number) => React.ReactNode;
}) {
  const numbers = useMemo(
    () => (props.spread ? [props.page, props.page + 1] : [props.page]),
    [props.page, props.spread],
  );

  if (props.mime !== 'application/pdf') {
    return <ImagePage file={props.file} rotation={props.rotation} onPages={props.onPages} renderOverlay={props.renderOverlay} />;
  }

  return (
    <div className="flex justify-center gap-2 overflow-auto">
      {numbers.map((number) => (
        <PdfPage key={number} {...props} page={number} />
      ))}
    </div>
  );
}

function PdfPage(props: Parameters<typeof PageStack>[0]) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);

  useEffect(() => {
    let cancelled = false;
    let task: { cancel: () => void } | null = null;

    const render = async (): Promise<void> => {
      try {
        const pdfjs = await import('pdfjs-dist');

        pdfjs.GlobalWorkerOptions.workerSrc = new URL('pdfjs-dist/build/pdf.worker.min.mjs', import.meta.url).toString();

        const document = await pdfjs.getDocument({ data: await props.file.arrayBuffer() }).promise;

        if (cancelled) {
          return;
        }

        props.onPages(document.numPages);

        if (props.page > document.numPages) {
          return;
        }

        const page = await document.getPage(props.page);
        const unscaled = page.getViewport({ scale: 1, rotation: props.rotation });
        const available = props.container.current?.clientWidth ?? 800;

        const scale = props.fit === 'width'
          ? (available - 24) / unscaled.width / (props.spread ? 2 : 1)
          : props.fit === 'page'
            ? Math.min((available - 24) / unscaled.width, (window.innerHeight - 200) / unscaled.height)
            : props.zoom;

        const viewport = page.getViewport({ scale, rotation: props.rotation });
        const context = canvas.current?.getContext('2d');

        if (context === null || context === undefined || canvas.current === null) {
          return;
        }

        canvas.current.width = viewport.width;
        canvas.current.height = viewport.height;
        setSize({ width: viewport.width, height: viewport.height });

        task = page.render({ canvasContext: context, viewport });
        await (task as unknown as { promise: Promise<void> }).promise;
      } catch (error) {
        if (! cancelled) {
          props.onFailure(
            error instanceof Error && /password/i.test(error.message)
              ? 'This PDF is password protected, so it cannot be shown here.'
              : 'This file could not be rendered.',
          );
        }
      }
    };

    void render();

    return () => {
      cancelled = true;
      task?.cancel();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.file, props.page, props.zoom, props.fit, props.rotation, props.spread]);

  return (
    <div className="relative">
      <canvas ref={canvas} className="max-w-full" />
      {size !== null && props.renderOverlay(props.page, size.width, size.height)}
    </div>
  );
}

function ImagePage(props: {
  file: Blob;
  rotation: number;
  onPages: (pages: number) => void;
  renderOverlay: (page: number, width: number, height: number) => React.ReactNode;
}) {
  const [url, setUrl] = useState<string | null>(null);
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);

  useEffect(() => {
    const objectUrl = URL.createObjectURL(props.file);
    setUrl(objectUrl);
    props.onPages(1);

    return () => URL.revokeObjectURL(objectUrl);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.file]);

  return (
    <div className="relative flex justify-center">
      {url !== null && (
        <img
          src={url}
          alt="Sheet"
          className="max-w-full"
          style={{ transform: `rotate(${props.rotation}deg)` }}
          onLoad={(event) => setSize({ width: event.currentTarget.clientWidth, height: event.currentTarget.clientHeight })}
        />
      )}
      {size !== null && props.renderOverlay(1, size.width, size.height)}
    </div>
  );
}
