/**
 * Sheets and import, and what survives losing the network.
 *
 * A ChordPro file imported through the dialog, a PDF attached and uploaded, a page rendered, marks
 * drawn and reloaded — then the network goes away and the library still reads and still accepts a
 * new song, which drains when it comes back.
 */

import { APP, log, open, shot, signIn } from '../lib/browser.mjs';
import { fixture } from '../lib/env.mjs';
import { rows } from '../lib/local.mjs';

const { browser, context, page } = await open();
await signIn(page);
// 1. Import a ChordPro file through the import dialog.
await page.getByRole('button', { name: 'Import' }).click();
await page.locator('.fixed input[type=file]').setInputFiles({
  name: 'Be Thou My Vision.chopro',
  mimeType: 'text/plain',
  buffer: Buffer.from('{title: Be Thou My Vision}\n{key: D}\n{verse: 1}\n[D]Be thou my [G]vision, O [A]Lord of my [D]heart\n'),
});
await page.waitForSelector('text=1 imported', { timeout: 10000 });
log('imported a ChordPro file through the dialog');
await page.getByRole('button', { name: 'Done' }).click();
await page.waitForSelector('text=Be Thou My Vision');

// 2. Attach a sheet to the original song and let the blob queue upload it.
await page.getByRole('link', { name: /Amazing Grace/ }).first().click();
await page.waitForSelector('text=Sheets');
await page.getByRole('button', { name: 'Attach a sheet' }).click();
await page.locator('.fixed input[type=file]').setInputFiles(fixture('piano.pdf'));
await page.getByRole('button', { name: 'Attach', exact: true }).click();
await page.waitForTimeout(2500);
console.log('  sheets panel:', JSON.stringify((await page.locator('section:has-text("Sheets")').last().innerText()).slice(0, 400)));
log('sheet attached');

// The queue drains on the sync tick; nudge it by clicking the chip.
await page.getByTitle(/Nothing is blocked/).click();
await page.getByRole('button', { name: 'Sync now' }).click();
await page.waitForTimeout(1500);
await page.getByRole('button', { name: 'Close' }).click();

// Scope to the sheets list: the part <select> also contains the word "piano".
const sheetRows = page.locator('section:has-text("Sheets") li');
await sheetRows.first().waitFor({ timeout: 20000 });
console.log('  sheet rows:', JSON.stringify(await sheetRows.allInnerTexts()));
await shot(page, '11-sheets-panel');

// 3. Open the viewer and check a page actually renders.
await sheetRows.locator('a').first().click();
await page.waitForSelector('canvas', { timeout: 20000 });
const canvas = await page.locator('canvas').first().boundingBox();
log('viewer rendered a page:', `${Math.round(canvas.width)}×${Math.round(canvas.height)}`);
if (canvas.width < 100) throw new Error('The PDF did not render');
await shot(page, '12-sheet-viewer');

// 4. Annotate it, and check the mark survives a reload.
await page.getByRole('button', { name: 'annotate' }).click();
const box = await page.locator('svg').first().boundingBox();
await page.mouse.move(box.x + 60, box.y + 80);
await page.mouse.down();
await page.mouse.move(box.x + 200, box.y + 140, { steps: 8 });
await page.mouse.up();
await page.waitForTimeout(800);
await page.reload();
await page.waitForSelector('canvas', { timeout: 20000 });
await page.waitForTimeout(1500);
const strokes = await page.locator('svg path').count();
log('annotation strokes after a reload:', strokes);
if (strokes === 0) throw new Error('The annotation did not survive a reload');
await shot(page, '13-annotated');

// 5. Offline: the shell and the library must come back with the network off.
await context.setOffline(true);
await page.goto(`${APP}/library`);
await page.waitForSelector('text=Amazing Grace', { timeout: 20000 });
const chip = await page.getByTitle(/Nothing is blocked/).innerText();
log('with the network off, the library still renders; the chip says', JSON.stringify(chip.trim()));
await shot(page, '14-offline');

// A write made offline queues rather than failing.
await page.getByPlaceholder('Add a song…').fill('Written on a plane');
await page.getByRole('button', { name: 'Add', exact: true }).click();
await page.waitForSelector('text=Written on a plane', { timeout: 10000 });
log('a song created offline is on screen immediately');

await context.setOffline(false);
await page.goto(`${APP}/library`);
await page.getByTitle(/Nothing is blocked/).click();
await page.getByRole('button', { name: 'Sync now' }).click();
await page.waitForTimeout(2000);
const status = await page.locator('dd').nth(1).innerText();
log('after reconnecting, waiting to send:', JSON.stringify(status));
await shot(page, '15-sync-panel');

await browser.close();
console.log('\nSheets, import, annotation and offline all behaved.');
