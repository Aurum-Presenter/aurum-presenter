/**
 * Ending a session closes its screens.
 *
 * The audience and stage windows close themselves, a log entry is written, and the hold that keeps a
 * downloaded update from reloading the app mid-song is released.
 */

import { log, open, signIn } from '../lib/browser.mjs';
import { databases } from '../lib/local.mjs';

const { browser, context, page } = await open();
await signIn(page);

await page.getByRole('link', { name: 'Sets', exact: true }).click();
await page.getByRole('link', { name: 'Sunday morning' }).click();
await page.getByRole('button', { name: 'Present' }).click();
await page.waitForSelector('text=Running order', { timeout: 15000 });

const [audience] = await Promise.all([
  context.waitForEvent('page'),
  page.getByRole('button', { name: 'Audience window' }).click(),
]);
await audience.waitForTimeout(1500);
log('the audience window is open and showing:', JSON.stringify((await audience.locator('body').innerText()).slice(0, 40)));

const [stage] = await Promise.all([
  context.waitForEvent('page'),
  page.getByRole('button', { name: 'Stage window' }).click(),
]);
await stage.waitForTimeout(1500);
log('the stage window is open');

const sessionId = new URL(page.url()).pathname.split('/').pop();

await page.getByRole('button', { name: 'End', exact: true }).click();
await page.getByRole('button', { name: 'End session' }).click();
await page.waitForTimeout(2500);

log('after ending, the audience window is closed:', audience.isClosed());
log('after ending, the stage window is closed:', stage.isClosed());
if (! audience.isClosed()) throw new Error('The audience window stayed open');
if (! stage.isClosed()) throw new Error('The stage window stayed open');

const logged = await page.evaluate(async (session) => {
  const names = (await indexedDB.databases()).map((d) => d.name ?? '').filter((n) => n.startsWith('aurum-'));
  for (const name of names) {
    const entry = await new Promise((resolve) => {
      const open = indexedDB.open(name);
      open.onsuccess = () => {
        const database = open.result;
        if (! database.objectStoreNames.contains('session_log')) { database.close(); resolve(null); return; }
        const request = database.transaction('session_log', 'readonly').objectStore('session_log').get(session);
        request.onsuccess = () => { database.close(); resolve(request.result ?? null); };
        request.onerror = () => { database.close(); resolve(null); };
      };
    });
    if (entry !== null) return entry;
  }
  return null;
}, sessionId);

log('a session log entry was written:', logged === null ? 'no' : `${logged.set_name}, ended ${logged.ended_at}, ${logged.events.length} advance(s)`);
if (logged === null || logged.ended_at === null) throw new Error('No completed session log entry');

const held = await page.evaluate(() => localStorage.getItem('aurum.session.active'));
log('the update hold is released:', held === null);
if (held !== null) throw new Error('Updates are still held after the session ended');

await browser.close();
console.log('\nEnding a session closes the outputs, logs it, and lets an update in again.');
