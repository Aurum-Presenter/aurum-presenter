/**
 * Two devices editing the same field, and the panel that offers the loser back.
 *
 * Nothing is destroyed silently: the later write wins, the displaced value is kept, and a musician
 * can restore it from the sync panel.
 */

import { Api, uuidv7 } from '../lib/api.mjs';
import { APP, log, open, shot, signIn } from '../lib/browser.mjs';
import { state } from '../lib/env.mjs';

const world = state();

// Make the conflict this spec is about: one device writes a note, a second writes over it from a
// stale base. The later write wins and the server keeps what it displaced.
const device = new Api();
await device.login(world.account.email, world.account.password, world.account.secret);

await device.push(world.workspace, [{
  op_id: uuidv7(),
  table: 'songs',
  record_id: world.song,
  op: 'upsert',
  payload: { notes: 'written by the first device' },
}]);

await device.push(world.workspace, [{
  op_id: uuidv7(),
  table: 'songs',
  record_id: world.song,
  op: 'upsert',
  payload: { notes: 'written by the second device' },
  base_updated_at: '2020-01-01T00:00:00.000Z',
}]);

log('two devices have edited the same note');

const { browser, page } = await open();
await signIn(page);
// The sync panel is the way in, exactly as a musician would find it.
await page.getByTitle(/Nothing is blocked/).click();
await page.getByRole('link', { name: 'Conflicts' }).click();
await page.waitForSelector('text=songs.notes', { timeout: 15000 });

const first = await page.locator('li').first().innerText();
log('conflict shown:', JSON.stringify(first.replace(/\n+/g, ' | ').slice(0, 160)));
await shot(page, '18-conflicts');

await page.getByRole('button', { name: 'Put the overwritten value back' }).first().click();
await page.waitForTimeout(1500);

// The restored value should now be the song's, locally and after a push.
await page.getByTitle(/Nothing is blocked/).click();
await page.getByRole('button', { name: 'Sync now' }).click();
await page.waitForTimeout(2000);
await page.getByRole('button', { name: 'Close' }).click();

await page.goto(`${APP}/library`);
await page.getByRole('link', { name: /Amazing Grace/ }).first().click();
await page.getByRole('button', { name: 'Details' }).click();
const notes = await page.locator('textarea').inputValue();
log('the song\'s notes after restoring:', JSON.stringify(notes));
if (! notes.includes('first device')) throw new Error('The overwritten value was not restored');

await browser.close();
console.log('\nConflict review works.');
