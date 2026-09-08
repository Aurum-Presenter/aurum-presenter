/**
 * Replacing a sheet's file with one that has a different number of pages.
 *
 * Marks are stored at coordinates normalised to a page, so they survive a zoom and a rotate but not
 * the pages moving underneath them. They are kept, and the viewer says they may not line up.
 */

import { log, open, shot, signIn } from '../lib/browser.mjs';
import { fixture, state } from '../lib/env.mjs';
import { rows } from '../lib/local.mjs';

const world = state();

const { browser, page } = await open();
await signIn(page);
// A song of its own, so the run is repeatable.
const title = `Replace test ${Date.now()}`;
await page.getByPlaceholder('Add a song…').fill(title);
await page.getByRole('button', { name: 'Add', exact: true }).click();
await page.waitForSelector('text=Sheets', { timeout: 15000 });

await page.getByRole('button', { name: 'Attach a sheet' }).click();
await page.locator('.fixed input[type=file]').setInputFiles(fixture('piano.pdf'));
await page.getByRole('button', { name: 'Attach', exact: true }).click();
await page.waitForTimeout(1500);
log('attached a one-page sheet');

await page.getByRole('link', { name: /lead|piano|other/ }).first().click();
await page.waitForSelector('text=annotate', { timeout: 15000 });
await page.getByRole('button', { name: 'annotate' }).click();

// Draw a mark, and do not go on until the device has actually stored one: the canvas has to be
// rendered before a drag lands on it, and a spec that assumes otherwise fails for the wrong
// reason.
await page.waitForSelector('canvas', { timeout: 15000 });

const sheetId = page.url().split('/').pop();

for (let attempt = 0; attempt < 3; attempt++) {
  const box = await page.locator('canvas').last().boundingBox();

  await page.mouse.move(box.x + 80, box.y + 120);
  await page.mouse.down();
  await page.mouse.move(box.x + 220, box.y + 200, { steps: 12 });
  await page.mouse.up();
  await page.waitForTimeout(1200);

  const marks = (await rows(page, world.workspace, 'annotations'))
    .filter((mark) => mark.sheet_id === sheetId);

  if (marks.length > 0) {
    break;
  }

  if (attempt === 2) {
    throw new Error('The mark would not save — the viewer never accepted a stroke.');
  }
}

await page.getByRole('button', { name: 'done' }).click();
log('drew a mark on it')

await page.goBack();
await page.waitForSelector('text=Sheets', { timeout: 10000 });
await page.locator('input[type=file]').first().setInputFiles(fixture('piano-2page.pdf'));
await page.waitForTimeout(2500);
log('replaced the file with a two-page one');

await page.getByRole('link', { name: /lead|piano|other/ }).first().click();
await page.waitForSelector('text=annotate', { timeout: 15000 });
await page.waitForTimeout(2000);


const body = await page.locator('body').innerText();
const flagged = body.includes('may not line up');
log('the viewer flags the marks:', flagged);
await shot(page, '23-may-not-line-up');
if (! flagged) throw new Error('The replaced sheet did not flag its older marks');

const kept = (await rows(page, world.workspace, 'annotations'))
  .filter((mark) => mark.sheet_id === sheetId && mark.deleted_at === null).length;
log('marks still on the device:', kept);
if (kept < 1) throw new Error('The marks were destroyed by the replace');

await browser.close();
console.log('\nA replaced file keeps the marks and says they may not line up.');
