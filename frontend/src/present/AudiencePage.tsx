import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { backgroundUrl } from './background';
import { audienceSlide, DEFAULT_THEME, type SessionState } from './session';
import { goFullscreen } from './displays';
import { AudienceSlide } from './SlideView';
import { OutputTransport } from './transport';

/**
 * The audience screen.
 *
 * It renders the session state and nothing else: no database writes, no sync worker, no logic of
 * its own. That is what lets it be reloaded, moved between screens, or cast, and come back on
 * the right slide without being told anything.
 *
 * It never shows a spinner or a flash of white — before the first message arrives it paints the
 * theme background, because a white rectangle in front of a congregation is worse than a blank
 * one.
 */
export function AudiencePage() {
  const [params] = useSearchParams();
  const sessionId = params.get('session') ?? '';
  const [state, setState] = useState<SessionState | null>(null);

  useEffect(() => {
    const transport = new OutputTransport(sessionId, 'audience', screenLabel(), setState);

    // The first paint comes from the local mirror, so a window reopened mid-service shows the
    // current slide before the control surface has said anything.
    void (async () => {
      const { WorkspaceDb } = await import('../db/schema');
      const workspaceId = params.get('workspace');

      if (workspaceId === null) {
        return;
      }

      const db = new WorkspaceDb(workspaceId);
      const record = await db.live_sessions.get(sessionId);

      if (record !== undefined) {
        setState((current) => current ?? (record.state as SessionState));
      }

      db.close();
    })();

    return () => transport.close();
  }, [sessionId, params]);

  useEffect(() => {
    const key = (event: KeyboardEvent): void => {
      if (event.key === 'f' || event.key === 'F') {
        goFullscreen();
      }
    };

    window.addEventListener('keydown', key);

    return () => window.removeEventListener('keydown', key);
  }, []);

  const theme = state?.theme ?? DEFAULT_THEME;
  const slide = state === null ? null : audienceSlide(state);
  const image = useBackground(state);

  return (
    <div
      className="h-dvh w-dvw overflow-hidden"
      style={{
        // Business rule 2: a background that is not on this device falls back to the theme's
        // colour. A congregation should never be shown a broken image or a white rectangle.
        background: image === null ? colourOf(theme) : `${colourOf(theme)} center / cover no-repeat url(${image})`,
        color: theme.text_color,
      }}
    >
      {state !== null && (
        <AudienceSlide slide={slide} theme={theme} workspaceId={state.workspace_id} />
      )}

      {state?.message != null && (
        <div className="absolute inset-x-0 bottom-0 bg-black/70 p-6 text-center" style={{ fontSize: '5vh' }}>
          {state.message}
        </div>
      )}
    </div>
  );
}

/**
 * The background image, read from this device's copy and only fetched if it is missing. It is
 * loaded here rather than passed in state so that every output resolves it for itself.
 */
function useBackground(state: SessionState | null): string | null {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    if (state === null || state.theme.background_kind !== 'image') {
      setUrl(null);
      return;
    }

    let cancelled = false;
    let objectUrl: string | null = null;

    void (async () => {
      const { WorkspaceDb } = await import('../db/schema');
      const db = new WorkspaceDb(state.workspace_id);
      const found = await backgroundUrl(db, state.workspace_id, state.theme.background_value);
      db.close();

      if (cancelled) {
        return;
      }

      objectUrl = found;
      setUrl(found);
    })();

    return () => {
      cancelled = true;

      if (objectUrl !== null) {
        URL.revokeObjectURL(objectUrl);
      }
    };
  }, [state?.theme.background_kind, state?.theme.background_value, state?.workspace_id]);

  return url;
}

/** The colour behind everything, whatever the theme's background kind is. */
function colourOf(theme: SessionState['theme']): string {
  return theme.background_kind === 'color' || theme.background_kind === 'gradient'
    ? theme.background_value
    : '#000000';
}

function screenLabel(): string {
  return `Audience · ${window.screen.width}×${window.screen.height}`;
}
