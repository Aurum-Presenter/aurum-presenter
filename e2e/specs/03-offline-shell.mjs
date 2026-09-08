/**
 * A cold start with no network.
 *
 * The service worker has to answer for a route nobody visited online: the app is opened, the worker
 * takes over, the tab is closed, the network is cut, and the app must still reach the library and
 * navigate to a route it has never fetched.
 */

import { APP, log, open, signIn } from '../lib/browser.mjs';
import { state } from '../lib/env.mjs';

const { browser, context, page } = await open();
await signIn(page);
await page.waitForTimeout(3000);

console.log('registration:', await page.evaluate(async () => {
  const [registration] = await navigator.serviceWorker.getRegistrations();
  return registration === undefined ? 'none' : {
    scope: registration.scope,
    installing: registration.installing?.state ?? null,
    waiting: registration.waiting?.state ?? null,
    active: registration.active?.state ?? null,
  };
}));

await page.reload();
await page.waitForTimeout(2500);
console.log('controller after reload:', await page.evaluate(() => navigator.serviceWorker.controller?.scriptURL ?? null));

await context.setOffline(true);

// A cold launch with no connection: new page, no memory of anything but what is on the device.
const cold = await context.newPage();
await cold.goto(`${APP}/library`);
await cold.waitForTimeout(2500);
console.log('offline cold start:', JSON.stringify((await cold.locator('body').innerText()).slice(0, 120).replace(/\n/g, ' | ')));

await cold.goto(`${APP}/sets`);
await cold.waitForTimeout(1500);
console.log('offline navigation to /sets:', JSON.stringify((await cold.locator('body').innerText()).slice(0, 90).replace(/\n/g, ' | ')));

await context.setOffline(false);
await browser.close();
