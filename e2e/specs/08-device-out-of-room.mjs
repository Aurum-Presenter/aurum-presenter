/**
 * A device with no room left.
 *
 * Every write to the file store fails the way a full disk does. The app must name the file it could
 * not keep, list what is held on purpose, and let one be released — without ever throwing away a pin
 * by itself.
 */

import { APP, log, open, shot, signIn } from '../lib/browser.mjs';

const { browser, context, page } = await open();

// A device with no room left: every write to the origin private file system fails the way a full
// disk does.
await context.addInitScript(() => {
  const full = () => {
    const error = new Error('The quota has been exceeded.');
    error.name = 'QuotaExceededError';
    throw error;
  };

  Object.defineProperty(navigator.storage, 'getDirectory', {
    configurable: true,
    value: async () => ({
      getDirectoryHandle: async () => ({
        getFileHandle: async () => ({
          getFile: async () => { throw new Error('not here'); },
          createWritable: async () => ({ write: full, close: async () => undefined }),
        }),
        removeEntry: async () => undefined,
      }),
    }),
  });
});

await page.reload();
await signIn(page);
await page.waitForSelector('text=This device is out of room', { timeout: 60000 });
const said = await page.locator('.fixed').innerText();
log('the dialog says:\n   ' + said.split('\n').join('\n   '));

if (! /needs \d+ MB/.test(said)) throw new Error('The dialog did not name the file that would not fit');
await shot(page, '21-out-of-room');

// Releasing a pin is the user's decision, and it is offered here.
const release = page.getByRole('button', { name: 'Release' }).first();
if (await release.count() === 0) throw new Error('No pin was offered for release');
await release.click();
await page.waitForTimeout(1500);
log('after releasing a pin the dialog closes:', await page.locator('text=This device is out of room').count() === 0);

// The release is a decision about this device, and it can be undone from the storage page.
await page.goto(`${APP}/settings/storage`);
await page.waitForSelector('text=Released on this device', { timeout: 15000 });
log('the storage page lists it as released, and offers it back');
await shot(page, '22-released');
await page.getByRole('button', { name: 'Keep it again' }).first().click();
await page.waitForTimeout(1000);
if (await page.locator('text=Released on this device').count() !== 0) throw new Error('Keeping it again did not take');
log('kept again');

await browser.close();
console.log('\nA full device names what it could not keep, and never throws away a pin by itself.');
