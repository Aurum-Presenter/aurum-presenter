import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * Where the suite finds the stack and its own files.
 *
 * Everything is overridable by environment variable so the same specs can be pointed at a
 * different build — which is the whole reason this suite exists: it is the oracle the Rust
 * rewrite is judged against, and it must not know or care which implementation is answering.
 */
export const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));

// One origin by default: the API binary serves the client's assets, so the app and the API are
// the same host and port. `AURUM_APP=http://127.0.0.1:4174` points the suite at Trunk's dev
// server instead, which is what a client change is worked against.
export const APP = process.env.AURUM_APP ?? 'http://127.0.0.1:8080';
export const API = process.env.AURUM_API ?? 'http://127.0.0.1:8080';

/** The browser Playwright drives. Pre-installed in CI images; overridable everywhere else. */
export const CHROME = process.env.CHROME_PATH
  ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';

export const STATE_FILE = join(ROOT, '.state.json');

export function fixture(name) {
  return join(ROOT, 'fixtures', name);
}

export function shotsDir() {
  const directory = process.env.SHOTS ?? join(ROOT, 'shots');

  if (! existsSync(directory)) {
    mkdirSync(directory, { recursive: true });
  }

  return directory;
}

/**
 * What `seed.mjs` left behind: the account it created, its two-factor secret, and the ids of the
 * song and set every spec reads. A spec that runs without it should say so plainly rather than
 * failing somewhere deep in a sign-in.
 */
export function state() {
  if (! existsSync(STATE_FILE)) {
    throw new Error('No .state.json — run `node lib/seed.mjs` (or `make e2e`, which seeds first).');
  }

  return JSON.parse(readFileSync(STATE_FILE, 'utf8'));
}

export const log = (...args) => console.log('·', ...args);
