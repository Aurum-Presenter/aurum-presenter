/**
 * A sheet, attached and read on one device with nothing behind it.
 *
 * The rules this covers only exist because a sheet is two things travelling separately: a row
 * that syncs, and a file that does not. Attaching reads the page count from the file itself, the
 * viewer renders from the local copy rather than streaming, and a mark is stored in page
 * coordinates (business rule 7) so it lands in the same place at any zoom and after a reload.
 *
 * Like specs 13 and 14 this needs no server — which is the point: everything here has to work on
 * a stage with no signal.
 */

import { log, open, shot } from '../lib/browser.mjs';
import { APP, fixture } from '../lib/env.mjs';

const { browser, context, page } = await open();

await page.goto(APP);
await page.getByRole('button', { name: /use it on this device without an account/ })
  .click({ timeout: 20000 });

await page.getByPlaceholder('Add a song…').fill('Cornerstone');
await page.getByPlaceholder('Add a song…').press('Enter');
await page.waitForSelector('text=What key is this chart written in?', { timeout: 15000 });
await page.getByRole('button', { name: 'G', exact: true }).click();

await page.getByRole('button', { name: 'Attach a sheet' }).click();
await page.locator('input[type=file]').first().setInputFiles(fixture('piano-2page.pdf'));
await page.getByRole('button', { name: 'Attach', exact: true }).click();

// The page count is read from the file on the device that attached it, not waited for from a
// server that has not been told about it yet.
await page.waitForSelector('text=2 pages', { timeout: 30000 });
log('the sheet was attached and its pages counted here');

await page.getByRole('link', { name: /lead/ }).first().click();
// An unpainted canvas is 300×150 of transparent black, which is easy to mistake for a page full
// of ink — and a canvas mid-render is white before it is anything else. So this waits for a real
// size, opaque pixels, and some of them dark, rather than sampling once and hoping.
const inspect = () => page.evaluate(() => {
  const canvas = document.querySelector('canvas');

  if (canvas === null || canvas.width < 400) {
    return { width: 0, height: 0, opaque: 0, ink: 0 };
  }

  const pixels = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
  let ink = 0;
  let opaque = 0;

  for (let at = 0; at < pixels.length; at += 4) {
    if (pixels[at + 3] < 200) continue;

    opaque++;

    if (pixels[at] < 200 || pixels[at + 1] < 200 || pixels[at + 2] < 200) ink++;
  }

  return { width: canvas.width, height: canvas.height, opaque, ink };
});

await page.waitForSelector('canvas', { timeout: 30000 });

let drawn = await inspect();

for (let attempt = 0; attempt < 60 && drawn.ink < 500; attempt++) {
  await page.waitForTimeout(500);
  drawn = await inspect();
}

log(`rendered ${drawn.width}×${drawn.height}, ${drawn.opaque} painted, ${drawn.ink} of them dark`);

if (drawn.opaque < drawn.width * drawn.height * 0.9) {
  throw new Error('The page was not painted');
}

if (drawn.ink < 500) {
  throw new Error('The page rendered blank');
}

await shot(page, '15-sheet');

// A mark, drawn across the page.
await page.getByRole('button', { name: 'annotate', exact: true }).click();

const surface = page.locator('svg').first();
const box = await surface.boundingBox();

await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.2);
await page.mouse.down();
await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.4, { steps: 8 });
await page.mouse.up();

await page.waitForFunction(() => document.querySelectorAll('svg path').length > 0, {
  timeout: 10000,
});

const before = await page.locator('svg path').first().getAttribute('d');
log('drew a mark');

// It is stored on the page, not on the screen: the same reload brings it back unmoved.
await page.reload();
await page.waitForSelector('svg path', { timeout: 30000 });

const after = await page.locator('svg path').first().getAttribute('d');

if (before !== after) {
  throw new Error(`The mark moved across a reload:\n  ${before}\n  ${after}`);
}

log('the mark came back in the same place');

// And zooming moves it with the music rather than leaving it behind in pixels.
await page.getByRole('button', { name: '+', exact: true }).click();
await page.waitForFunction(
  (was) => document.querySelector('svg path')?.getAttribute('d') !== was,
  before,
  { timeout: 15000 },
);

log('and it scales with the page');
await shot(page, '15-annotated');

log('OK');

await context.close();
await browser.close();
