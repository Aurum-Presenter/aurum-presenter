import { useEffect } from 'react';

/**
 * Keeps the screen awake, and lets it sleep again on the way out.
 *
 * Every screen this is used on is one somebody is looking at without touching: a chart on a
 * music stand, a set in reader mode, a projector showing a slide through a long prayer. A
 * dimmed phone mid-song is the same failure as a lost slide (PWA business rule 4).
 *
 * The lock is dropped whenever the tab is hidden, so it is asked for again on the way back.
 */
export function useWakeLock(): void {
  useEffect(() => {
    let lock: { release: () => Promise<void> } | null = null;
    let cancelled = false;

    const request = async (): Promise<void> => {
      try {
        const api = (navigator as { wakeLock?: { request: (kind: 'screen') => Promise<{ release: () => Promise<void> }> } }).wakeLock;

        if (api === undefined || cancelled) {
          return;
        }

        lock = await api.request('screen');
      } catch {
        // Denied, unsupported, or the tab was not visible. Everything still works.
      }
    };

    const onVisible = (): void => {
      if (document.visibilityState === 'visible') void request();
    };

    void request();
    document.addEventListener('visibilitychange', onVisible);

    return () => {
      cancelled = true;
      document.removeEventListener('visibilitychange', onVisible);
      void lock?.release().catch(() => undefined);
    };
  }, []);
}
