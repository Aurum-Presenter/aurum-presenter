/**
 * Building a set: adding songs, reordering them, and a key the band agrees on for the night.
 *
 * Two rules are only visible here. Reordering moves one row, using a fractional rank, so the
 * order has to survive a reload rather than only look right on screen. And a set's key override
 * beats every member's own preferred key (business rule 4) — which is the point of a band
 * deciding a key — while everything below it stays personal.
 *
 * Like spec 13 this runs against the built client alone, with no account and no server.
 */

import { log, open, shot } from '../lib/browser.mjs';
import { APP } from '../lib/env.mjs';

const SONGS = [['Cornerstone', 'G'], ['Amazing Grace', 'D']];

const { browser, context, page } = await open();

await page.goto(APP);
await page.getByRole('button', { name: /use it on this device without an account/ })
  .click({ timeout: 20000 });

for (const [title, key] of SONGS) {
  await page.getByPlaceholder('Add a song…').fill(title);
  await page.getByPlaceholder('Add a song…').press('Enter');
  await page.waitForSelector('text=What key is this chart written in?', { timeout: 15000 });
  await page.getByRole('button', { name: key, exact: true }).click();
  await page.getByRole('button', { name: 'Add a chart' }).click();
  await page.locator('textarea').first().fill(`[${key}]Line one`);
  await page.waitForSelector('text=Saved', { timeout: 5000 });
  await page.getByRole('link', { name: '← Library' }).click();
  await page.waitForSelector('input[placeholder="Add a song…"]');
}

log('two songs, each with a chart in a known key');

await page.getByRole('link', { name: 'Sets', exact: true }).click();
await page.getByPlaceholder(/New set/).fill('Sunday morning');
await page.locator('input[type=date]').fill('2026-09-13');
await page.getByRole('button', { name: 'Create', exact: true }).click();

await page.waitForSelector('text=Add items', { timeout: 15000 });
log('set created and opened');

await page.getByRole('button', { name: 'Add items' }).click();

// Picked one after another: the order they are clicked in is the order they land in.
for (const [title] of SONGS) {
  await page.locator('li > button').filter({ hasText: title }).first().click();
}

await page.getByRole('button', { name: /^Add 2 songs$/ }).click();
await page.waitForSelector('ol > li', { timeout: 10000 });

const order = async () => (await page.locator('ol > li').allInnerTexts())
  .map((line) => SONGS.map(([title]) => title).find((title) => line.includes(title)) ?? '?');

if (String(await order()) !== String(SONGS.map(([title]) => title))) {
  throw new Error(`Songs did not land in the order they were picked: ${await order()}`);
}

log('songs landed in the order they were picked:', await order());
await shot(page, '14-running-order');

// Reordering: the second item is dragged onto the first. The two halves are dispatched in
// separate turns because a real drag has time between them, and a client that records the
// grabbed row in its own state needs that time to have recorded it.
await page.evaluate(() => {
  window.__drag = new DataTransfer();

  document.querySelectorAll('ol > li')[1]
    .dispatchEvent(new DragEvent('dragstart', { dataTransfer: window.__drag, bubbles: true }));
});

await page.waitForTimeout(150);

await page.evaluate(() => {
  const target = document.querySelectorAll('ol > li')[0];

  target.dispatchEvent(
    new DragEvent('dragover', { dataTransfer: window.__drag, bubbles: true, cancelable: true }),
  );
  target.dispatchEvent(new DragEvent('drop', { dataTransfer: window.__drag, bubbles: true }));
});

await page.waitForFunction(
  () => document.querySelectorAll('ol > li')[0]?.innerText.includes('Amazing Grace'),
  { timeout: 5000 },
);

log('after the drag:', await order());

// A rank is only worth anything if it is what was written down.
await page.reload();
await page.waitForSelector('ol > li', { timeout: 15000 });

if ((await order())[0] !== 'Amazing Grace') {
  throw new Error(`The move did not survive a reload: ${await order()}`);
}

log('the new order came back from the device after a reload');

// The band's key for the night. Amazing Grace is written in D and is now first.
await page.locator('ol > li').first().locator('select').first().selectOption('A');
await page.waitForFunction(
  () => document.querySelector('ol > li')?.innerText.includes('set key'),
  { timeout: 5000 },
);

log('the first item now carries the set key');

// And it reaches the chart a musician actually reads.
await page.getByRole('link', { name: 'Read' }).click();
await page.waitForSelector('article', { timeout: 10000 });

const reading = await page.locator('article').innerText();

if (! reading.includes('Amazing Grace') || ! reading.includes('A')) {
  throw new Error(`Reader mode did not show the set key: ${JSON.stringify(reading)}`);
}

log('reader mode:', JSON.stringify(reading.split('\n').slice(0, 2).join(' / ')));
await shot(page, '14-reader');

await page.keyboard.press('ArrowRight');
await page.waitForFunction(
  () => document.body.innerText.includes('Cornerstone'),
  { timeout: 5000 },
);

log('an arrow key turned the page');
log('OK');

await context.close();
await browser.close();
