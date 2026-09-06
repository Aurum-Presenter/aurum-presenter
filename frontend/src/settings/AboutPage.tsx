import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { BlobStore } from '../blobs/store';
import { isStandalone, useInstall } from '../pwa/install';

/**
 * What this is and what version of it is running.
 *
 * The version and build time are embedded at build time so a support question can be answered
 * without guessing, and this is also the permanent home of the install action for anyone who
 * dismissed the banner.
 */
declare const __APP_VERSION__: string;
declare const __BUILD_TIME__: string;

export function AboutPage() {
  const { installed, ios, available, install } = useInstall();
  const [worker, setWorker] = useState<'ready' | 'unsupported' | 'none'>('none');
  const [persisted, setPersisted] = useState<boolean | null>(null);

  useEffect(() => {
    if (! ('serviceWorker' in navigator)) {
      setWorker('unsupported');
      return;
    }

    void navigator.serviceWorker.getRegistration().then((registration) => setWorker(registration === undefined ? 'none' : 'ready'));
    void BlobStore.estimate().then(() => undefined);
    void navigator.storage?.persisted?.().then(setPersisted).catch(() => setPersisted(null));
  }, []);

  return (
    <div className="mx-auto max-w-2xl p-4 text-sm">
      <Link className="underline" to="/library">← Library</Link>
      <h2 className="mb-3 mt-3 text-2xl font-semibold">About Aurum Presenter</h2>

      <dl className="mb-6 space-y-1">
        <Row label="Version" value={__APP_VERSION__} />
        <Row label="Built" value={new Date(__BUILD_TIME__).toLocaleString()} />
        <Row label="Running as" value={isStandalone() ? 'an installed app' : 'a browser tab'} />
        <Row
          label="Offline shell"
          value={worker === 'ready'
            ? 'installed — the app opens with no network'
            : worker === 'unsupported'
              ? 'not available in this browser; the app works online only'
              : 'not installed yet'}
        />
        <Row label="Storage kept under pressure" value={persisted === true ? 'yes' : 'not granted'} />
      </dl>

      {! installed && (
        <div className="mb-6">
          <button
            className="rounded bg-slate-900 px-4 py-2 text-white dark:bg-slate-100 dark:text-slate-900"
            onClick={() => void install()}
            disabled={! available && ! ios}
          >
            Install this app
          </button>
          {! available && ios && (
            <p className="mt-2 text-slate-500">On iOS: Share, then “Add to Home Screen”.</p>
          )}
          {! available && ! ios && (
            <p className="mt-2 text-slate-500">
              This browser has not offered an install prompt. It may already be installed, or it
              may not support installing web apps.
            </p>
          )}
        </div>
      )}

      <p className="text-slate-500">
        Charts, sets and the library live on this device and sync when there is a connection.
        Nothing here needs the internet to work — that is the point of it.
      </p>

      <ul className="mt-4 space-y-1">
        <li><Link className="underline" to="/settings/storage">Offline storage</Link></li>
        <li><Link className="underline" to="/settings/sync/conflicts">Conflicts</Link></li>
        <li><Link className="underline" to="/settings/trash">Trash</Link></li>
      </ul>
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex gap-2">
      <dt className="w-56 text-slate-500">{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
