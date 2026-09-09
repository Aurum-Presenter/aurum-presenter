/**
 * Pasting a chart that is written as chords over lyrics.
 *
 * Business rule 1: the paste is converted to ChordPro once, on the way in, and the original is
 * kept for exactly one undo. Business rule 5: a chart typed on a device with no account is a
 * chart — which is why this spec needs no server at all, and why it is the one spec that runs
 * against a stack that is only the built client.
 *
 * Nothing here knows how the conversion is implemented. It types into the editor a musician
 * types into and reads what the screen says back.
 */

import { log, open, shot } from '../lib/browser.mjs';
import { APP } from '../lib/env.mjs';

const PASTED = 'G           C\nChrist alone, cornerstone';

const { browser, context, page } = await open();

await page.goto(APP);
await page.getByRole('button', { name: /use it on this device without an account/ })
  .click({ timeout: 20000 });

await page.getByPlaceholder('Add a song…').fill('Cornerstone');
await page.getByPlaceholder('Add a song…').press('Enter');

await page.waitForSelector('text=What key is this chart written in?', { timeout: 15000 });
log('a new song asks what key its chart is written in rather than guessing');

await page.getByRole('button', { name: 'G', exact: true }).click();
await page.getByRole('button', { name: 'Add a chart' }).click();

const editor = page.locator('textarea').first();
await editor.waitFor({ timeout: 10000 });

// A real paste, with a clipboard payload — the editor decides what to do with it, and the
// browser's own paste must not also land.
await page.evaluate((text) => {
  const area = document.querySelector('textarea');
  const data = new DataTransfer();

  area.focus();
  area.setSelectionRange(area.value.length, area.value.length);
  data.setData('text/plain', text);
  area.dispatchEvent(
    new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
  );
}, PASTED);

await page.waitForSelector('text=Converted from chords over lyrics.', { timeout: 5000 });
log('the paste was recognised and converted');

const converted = await editor.inputValue();

if (! converted.includes('[G]') || ! converted.includes('[C]')) {
  throw new Error(`The paste was not converted to ChordPro: ${JSON.stringify(converted)}`);
}

if (converted.includes(PASTED)) {
  throw new Error('The original text was left in the editor as well as the conversion');
}

await shot(page, '13-converted');

// The save is automatic and must not wait for the editor to be closed.
await page.waitForSelector('text=Saved', { timeout: 5000 });
log('the conversion saved itself');

// One undo, back to exactly what was pasted — not to what was there before it.
await page.getByRole('button', { name: 'Undo', exact: true }).click();

const undone = await editor.inputValue();

if (undone !== PASTED) {
  throw new Error(`Undo did not restore the paste: ${JSON.stringify(undone)}`);
}

log('undo put the pasted text back, unconverted');

// And it is one undo, not a history: the banner is gone.
if (await page.getByText('Converted from chords over lyrics.').count() > 0) {
  throw new Error('The conversion banner survived the undo');
}

await shot(page, '13-undone');

// Closing the editor reads the chart back in the key it was written in, from the device.
await page.getByRole('button', { name: 'Done', exact: true }).click();
await page.waitForSelector('text=Christ alone', { timeout: 10000 });
log('the chart reads back after the editor is closed');

log('OK');

await context.close();
await browser.close();
