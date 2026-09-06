import { useEffect, useState } from 'react';

/**
 * Installing the app.
 *
 * Installation is an upgrade, not a gate: the first visit works in a plain tab and nothing here
 * blocks anything. The banner appears on the third visit or the first pin, whichever comes
 * first, and a dismissal is respected for a month.
 */

const META_KEY = 'aurum.app';
const DISMISS_DAYS = 30;

interface AppMeta {
  visits: number;
  installed_at: string | null;
  dismissed_until: string | null;
  persist_granted: boolean;
}

const EMPTY: AppMeta = { visits: 0, installed_at: null, dismissed_until: null, persist_granted: false };

export function readMeta(): AppMeta {
  try {
    return { ...EMPTY, ...(JSON.parse(localStorage.getItem(META_KEY) ?? '{}') as Partial<AppMeta>) };
  } catch {
    return EMPTY;
  }
}

export function writeMeta(changes: Partial<AppMeta>): void {
  try {
    localStorage.setItem(META_KEY, JSON.stringify({ ...readMeta(), ...changes }));
  } catch {
    // Storage blocked. The app works; it will simply ask again next time.
  }
}

export function countVisit(): void {
  writeMeta({ visits: readMeta().visits + 1 });
}

export function isStandalone(): boolean {
  return window.matchMedia('(display-mode: standalone)').matches
    || (navigator as unknown as { standalone?: boolean }).standalone === true;
}

interface InstallPrompt extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

/** The install state, for both the banner and the About page. */
export function useInstall() {
  const [prompt, setPrompt] = useState<InstallPrompt | null>(null);
  const [installed, setInstalled] = useState(isStandalone());

  useEffect(() => {
    const onPrompt = (event: Event): void => {
      event.preventDefault();
      setPrompt(event as InstallPrompt);
    };

    const onInstalled = (): void => {
      setInstalled(true);
      writeMeta({ installed_at: new Date().toISOString() });
    };

    window.addEventListener('beforeinstallprompt', onPrompt);
    window.addEventListener('appinstalled', onInstalled);

    return () => {
      window.removeEventListener('beforeinstallprompt', onPrompt);
      window.removeEventListener('appinstalled', onInstalled);
    };
  }, []);

  return {
    installed,
    /** iOS has no prompt event; it needs a sentence of instructions instead. */
    ios: /iphone|ipad|ipod/i.test(navigator.userAgent) && ! installed,
    available: prompt !== null,
    install: async (): Promise<'accepted' | 'dismissed' | 'unavailable'> => {
      if (prompt === null) {
        return 'unavailable';
      }

      await prompt.prompt();
      const { outcome } = await prompt.userChoice;
      setPrompt(null);

      return outcome;
    },
  };
}

export function InstallBanner() {
  const { installed, ios, available, install } = useInstall();
  const [showIos, setShowIos] = useState(false);
  const meta = readMeta();

  const dismissed = meta.dismissed_until !== null && Date.parse(meta.dismissed_until) > Date.now();
  const earned = meta.visits >= 3;

  if (installed || dismissed || ! earned || (! available && ! ios)) {
    return null;
  }

  const dismiss = (): void => {
    writeMeta({ dismissed_until: new Date(Date.now() + DISMISS_DAYS * 86_400_000).toISOString() });
    setShowIos(false);
  };

  return (
    <>
      <div className="flex flex-wrap items-center gap-3 border-b border-sky-200 bg-sky-50 px-4 py-2 text-sm text-sky-900">
        <span>Install Aurum on this device so it opens like an app and starts without a network.</span>

        <button
          className="rounded bg-slate-900 px-3 py-1 text-white"
          onClick={() => (ios ? setShowIos(true) : void install().then((outcome) => outcome === 'dismissed' && dismiss()))}
        >
          Install
        </button>

        <button className="underline" onClick={dismiss}>Not now</button>
      </div>

      {showIos && (
        <div className="fixed inset-0 z-30 flex items-center justify-center bg-slate-900/50 p-6" onClick={() => setShowIos(false)}>
          <div className="w-80 rounded bg-white p-4 text-sm dark:bg-slate-900" onClick={(event) => event.stopPropagation()}>
            <h2 className="mb-2 font-semibold">Add to the Home Screen</h2>
            <ol className="mb-3 list-decimal space-y-1 pl-4 text-slate-600 dark:text-slate-300">
              <li>Tap the Share button in Safari.</li>
              <li>Choose “Add to Home Screen”.</li>
              <li>Open Aurum from the icon — it will work with no signal.</li>
            </ol>
            <button className="underline" onClick={dismiss}>Got it</button>
          </div>
        </div>
      )}
    </>
  );
}
