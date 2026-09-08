/**
 * A theme with an image behind the slide, and the three ways a device can meet it.
 *
 * The device that uploaded it paints from its own copy; a device without the bytes downloads them;
 * a device with neither falls back to the theme's colour rather than showing a room a broken
 * image.
 */

import { APP, log, open, shot, signIn } from '../lib/browser.mjs';
import { fixture, state } from '../lib/env.mjs';
import { databases, row, rows } from '../lib/local.mjs';

const { browser, context, page } = await open();
await signIn(page);
await page.getByRole('link', { name: 'Sets', exact: true }).click();
await page.getByRole('link', { name: 'Sunday morning' }).click();
await page.getByRole('button', { name: 'Present' }).click();
await page.waitForSelector('text=Running order', { timeout: 15000 });

await page.getByRole('button', { name: 'Theme' }).click();
await page.locator('input[type=file]').setInputFiles(fixture('background.png'));
await page.waitForTimeout(3000);

const kind = await page.evaluate(() => document.body.innerText.includes('Remove the image'));
log('theme now uses an image background:', kind);
if (! kind) throw new Error('The background image was not accepted');

await page.getByRole('button', { name: 'Close' }).click();

const url = new URL(page.url());
const sessionId = url.pathname.split('/').pop();
const workspaceId = await page.evaluate(async () => {
  const remembered = localStorage.getItem('aurum.workspace');
  if (remembered !== null) return remembered;
  const names = indexedDB.databases ? (await indexedDB.databases()).map((d) => d.name ?? '') : [];
  return (names.find((name) => name.startsWith('aurum-')) ?? '').slice('aurum-'.length);
});
log('the audience window will open workspace', workspaceId);

const audience = await context.newPage();
audience.on('console', (m) => console.log('  [audience]', m.type(), m.text().slice(0, 140)));
audience.on('response', (r) => { if (r.url().includes('/assets/')) console.log('  [audience http]', r.status(), r.url().slice(0, 90)); });
await audience.goto(`${APP}/output/audience?session=${sessionId}&workspace=${workspaceId}`);
await audience.waitForTimeout(3000);
console.log('  theme in the audience window:', await audience.evaluate(() => {
  const raw = localStorage.getItem('aurum.stage.last');
  return raw === null ? 'no cached state' : JSON.stringify(JSON.parse(raw).theme ?? {});
}));
console.log('  cached background rows:', await audience.evaluate(async () => {
  const request = indexedDB.databases ? await indexedDB.databases() : [];
  return request.map((d) => d.name).join(', ');
}));
const painted = await audience.evaluate(() => getComputedStyle(document.querySelector('#root > div')).backgroundImage);
log('audience background:', painted.startsWith('url(') ? 'the image is painted' : `no image (${painted})`);
await shot(audience, '19-audience-background');
if (! painted.startsWith('url(')) throw new Error('The audience did not paint the background image');

// A device that never uploaded the image downloads it: the row travels, the bytes follow.
const forget = (target) => target.evaluate(async (workspace) => {
  await new Promise((resolve) => {
    const open = indexedDB.open(`aurum-${workspace}`);
    open.onsuccess = () => {
      const database = open.result;
      const request = database.transaction('files', 'readwrite').objectStore('files').openCursor();
      request.onsuccess = () => {
        const cursor = request.result;
        if (cursor === null) { database.close(); resolve(); return; }
        if (String(cursor.key).startsWith('theme-background:')) cursor.delete();
        cursor.continue();
      };
    };
  });
}, workspaceId);

await forget(audience);
await audience.reload();
await audience.waitForTimeout(3500);
const downloaded = await audience.evaluate(() => getComputedStyle(document.querySelector('#root > div')).backgroundImage);
log('a device without the bytes downloads them:', downloaded.startsWith('url(') ? 'the image is painted' : `no image (${downloaded})`);
if (! downloaded.startsWith('url(')) throw new Error('The background image was not downloaded');

// With the network off and the bytes not on this device, it must fall back to the theme colour.
await forget(audience);
await context.setOffline(true);
const cold = await context.newPage();
await cold.goto(`${APP}/output/audience?session=${sessionId}&workspace=${workspaceId}`);
await cold.waitForTimeout(3000);
const fallback = await cold.evaluate(() => {
  const root = document.querySelector('#root > div') ?? document.body;
  return `${getComputedStyle(root).backgroundImage} on ${getComputedStyle(root).backgroundColor}`;
});
log('a device without the image and without a network falls back to:', fallback.startsWith('none') ? `the theme colour (${fallback})` : fallback);
await shot(cold, '20-audience-fallback');
if (! fallback.startsWith('none')) throw new Error('An offline device must fall back to the theme colour');
await context.setOffline(false);

await browser.close();
console.log('\nTheme backgrounds work, and degrade the way the spec says.');
