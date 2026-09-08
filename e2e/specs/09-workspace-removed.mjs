/**
 * Losing access to a workspace takes it off the device.
 *
 * A database and a pinned file are planted for a workspace this account is not in. Signing in must
 * remove both, and touch nothing belonging to the workspace it is in.
 */

import { APP, log, open, signIn } from '../lib/browser.mjs';

/** A workspace id this account has never been a member of. */
const REMOVED = '01a00000-0000-7000-8000-000000000abc';

const { browser, page } = await open();

await page.goto(`${APP}/`);

// Plant what a device would still be holding after being removed from a band: the workspace's
// database, and a pinned file in the origin private file system.
await page.evaluate(async (id) => {
  await new Promise((resolve) => {
    const open = indexedDB.open(`aurum-${id}`, 1);
    open.onupgradeneeded = () => open.result.createObjectStore('blobs', { keyPath: 'sheet_id' });
    open.onsuccess = () => { open.result.close(); resolve(); };
  });

  const root = await navigator.storage.getDirectory();
  const directory = await root.getDirectoryHandle(`aurum-${id}`, { create: true });
  const handle = await directory.getFileHandle('a-pinned-sheet', { create: true });
  const writable = await handle.createWritable();
  await writable.write(new Blob(['pretend pdf']));
  await writable.close();
}, REMOVED);

log('planted a database and a pinned file for a workspace this account is not in');

await signIn(page);
await page.waitForTimeout(2500);

const after = await page.evaluate(async (id) => {
  const names = (await indexedDB.databases()).map((d) => d.name ?? '');
  let files = 'gone';
  try {
    const root = await navigator.storage.getDirectory();
    await root.getDirectoryHandle(`aurum-${id}`);
    files = 'still there';
  } catch { /* gone */ }
  return { names, files };
}, REMOVED);

log('databases now:', after.names.join(', '));
log(`the removed workspace's files are:`, after.files);

if (after.names.includes(`aurum-${REMOVED}`)) throw new Error('The removed workspace was left on the device');
if (after.files !== 'gone') throw new Error('The removed workspace kept its pinned files');
if (! after.names.some((name) => name.startsWith('aurum-') && name !== `aurum-${REMOVED}`)) {
  throw new Error('The workspace this account does belong to was deleted too');
}

await page.waitForSelector('text=Amazing Grace', { timeout: 10000 });
log('the workspace this account is in is untouched, and still shows its songs');

await browser.close();
console.log('\nLosing access to a workspace takes it off the device, and nothing else with it.');
