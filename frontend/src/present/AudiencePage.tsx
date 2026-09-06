import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
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

  return (
    <div
      className="h-dvh w-dvw overflow-hidden"
      style={{
        background: theme.background_kind === 'image'
          ? `#000 center / contain no-repeat url(${theme.background_value})`
          : theme.background_value,
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

function screenLabel(): string {
  return `Audience · ${window.screen.width}×${window.screen.height}`;
}
