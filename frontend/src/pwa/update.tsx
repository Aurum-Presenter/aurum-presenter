import { useEffect, useState } from 'react';
import { useRegisterSW } from 'virtual:pwa-register/react';

/**
 * Applying a new version.
 *
 * A downloaded update waits. It waits longer if a live session is running, because a reload
 * mid-song is the worst thing this app could do — and the flag that says so is set by the
 * control surface and cleared when the session ends (business rule 3).
 */

const SESSION_FLAG = 'aurum.session.active';

export function holdUpdates(active: boolean): void {
  try {
    if (active) {
      localStorage.setItem(SESSION_FLAG, '1');
    } else {
      localStorage.removeItem(SESSION_FLAG);
    }
  } catch {
    // Without storage the hold cannot be recorded; the toast is still not automatic.
  }
}

export function updatesHeld(): boolean {
  try {
    return localStorage.getItem(SESSION_FLAG) === '1';
  } catch {
    return false;
  }
}

export function UpdateToast() {
  const { needRefresh: [needRefresh], updateServiceWorker } = useRegisterSW({
    onRegisteredSW(_url, registration) {
      // Business rule 2: check on foreground and every half hour while there is a connection.
      const check = (): void => {
        if (navigator.onLine) {
          void registration?.update().catch(() => undefined);
        }
      };

      const timer = setInterval(check, 30 * 60 * 1000);
      document.addEventListener('visibilitychange', () => document.visibilityState === 'visible' && check());

      return () => clearInterval(timer);
    },
  });

  const [held, setHeld] = useState(updatesHeld());

  useEffect(() => {
    const timer = setInterval(() => setHeld(updatesHeld()), 5000);

    return () => clearInterval(timer);
  }, []);

  if (! needRefresh || held) {
    return null;
  }

  return (
    <div className="fixed bottom-4 left-1/2 z-30 flex -translate-x-1/2 items-center gap-3 rounded-full bg-slate-900 px-4 py-2 text-sm text-white shadow-lg dark:bg-slate-100 dark:text-slate-900">
      <span>A new version is ready.</span>
      <button className="underline" onClick={() => void updateServiceWorker(true)}>Reload</button>
      <button className="opacity-70 underline" onClick={() => setHeld(true)}>Later</button>
    </div>
  );
}
