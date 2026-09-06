import { useCallback, useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { joinSession } from './pairing';
import { isValidCode, normaliseCode, type SessionMessage, type SessionState } from './session';
import { StagePage } from './StagePage';

/**
 * Joining a running session from a phone or a tablet.
 *
 * A failed join never touches the session that is running (stage-view failure behaviour), and a
 * join that succeeds and then drops keeps the last slide on screen while it tries again — for
 * five minutes, with backoff, because a musician cannot debug a network mid-song.
 */
const RETRY_FOR_MS = 5 * 60 * 1000;

export function JoinPage() {
  const [params] = useSearchParams();
  const [code, setCode] = useState(normaliseCode(params.get('code') ?? ''));
  const [state, setState] = useState<SessionState | null>(null);
  const [status, setStatus] = useState<'idle' | 'joining' | 'joined' | 'stale'>('idle');
  const [problem, setProblem] = useState<string | null>(null);

  const send = useRef<((message: SessionMessage) => void) | null>(null);
  const since = useRef(0);
  const revision = useRef(-1);

  /**
   * The control surface counts an output as responding by its acks, so a paired device says so
   * on every state and once a second in between — the same heartbeat a window on the control
   * device sends.
   */
  const ack = useCallback((): void => {
    send.current?.({
      type: 'ack',
      output_id: 'paired',
      kind: 'paired-stage',
      label: 'Paired device',
      revision: revision.current,
    });
  }, []);

  useEffect(() => {
    const timer = setInterval(() => {
      if (send.current !== null) {
        ack();
      }
    }, 1000);

    return () => clearInterval(timer);
  }, [ack]);

  const attempt = useCallback(async (value: string): Promise<void> => {
    setStatus('joining');
    setProblem(null);

    try {
      send.current = await joinSession(
        value,
        (message) => {
          // Business rule 5: a message that arrives out of order must not move the screen back.
          if (message.type === 'state' && message.state.revision >= revision.current) {
            revision.current = message.state.revision;
            setState(message.state);
            setStatus('joined');
            ack();
          }
        },
        () => setStatus('stale'),
      );

      setStatus('joined');
      since.current = Date.now();

      // Cache the last state so a reopened tab paints instantly rather than blankly.
      localStorage.setItem('aurum.stage.code', value);
    } catch (error) {
      setStatus('idle');
      setProblem(error instanceof Error ? error.message : 'That did not work.');
    }
  }, []);

  useEffect(() => {
    if (params.get('code') !== null && isValidCode(params.get('code')!)) {
      void attempt(normaliseCode(params.get('code')!));
    }
  }, [params, attempt]);

  // Rejoin with backoff while the session is presumably still running.
  useEffect(() => {
    if (status !== 'stale') {
      return;
    }

    if (since.current !== 0 && Date.now() - since.current > RETRY_FOR_MS) {
      return;
    }

    const timer = setTimeout(() => void attempt(code), 3000);

    return () => clearTimeout(timer);
  }, [status, code, attempt]);

  useEffect(() => {
    if (state !== null) {
      localStorage.setItem('aurum.stage.last', JSON.stringify(state));
    }
  }, [state]);

  // When the session ends this device goes back to the join screen rather than holding the last
  // slide: the service is over, and the tablet is a tablet again.
  useEffect(() => {
    if (state?.ended === true) {
      localStorage.removeItem('aurum.stage.last');
      setStatus('idle');
    }
  }, [state?.ended]);

  if (status === 'joined' || status === 'stale') {
    return (
      <StagePage
        external={{
          state: state ?? cachedState(),
          stale: status === 'stale',
          send: (message) => send.current?.(message),
        }}
      />
    );
  }

  return (
    <div className="mx-auto max-w-sm p-6">
      <h2 className="mb-2 text-xl font-semibold">Join a session</h2>
      <p className="mb-4 text-sm text-slate-500">
        Enter the six characters shown on the control screen. Both devices need to be on the same
        network and signed into this workspace.
      </p>

      <form
        onSubmit={(event) => { event.preventDefault(); if (isValidCode(code)) void attempt(normaliseCode(code)); }}
      >
        <input
          className="mb-3 w-full rounded border border-slate-300 px-3 py-3 text-center font-mono text-2xl tracking-widest uppercase dark:border-slate-700 dark:bg-slate-900"
          placeholder="4KJ9QP"
          maxLength={6}
          autoFocus
          value={code}
          onChange={(event) => setCode(normaliseCode(event.target.value))}
        />

        <button
          className="w-full rounded bg-slate-900 py-2 text-white disabled:opacity-40 dark:bg-slate-100 dark:text-slate-900"
          disabled={! isValidCode(code) || status === 'joining'}
        >
          {status === 'joining' ? 'Joining…' : 'Join'}
        </button>
      </form>

      {problem !== null && <p className="mt-3 text-sm text-red-700 dark:text-red-400">{problem}</p>}
    </div>
  );
}

function cachedState(): SessionState | null {
  try {
    return JSON.parse(localStorage.getItem('aurum.stage.last') ?? 'null') as SessionState | null;
  } catch {
    return null;
  }
}
