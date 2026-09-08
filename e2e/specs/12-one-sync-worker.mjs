/**
 * Three windows of the app, one sync worker.
 *
 * Library, control surface and stage are routinely open together. An edit made offline must be
 * pushed once when the network returns, not once per window.
 */

import { APP, log, open, signIn } from '../lib/browser.mjs';

const { browser, context, page } = await open();

/** Every push that carried operations, with the operation ids it carried. */
const pushes = [];

context.on('request', (request) => {
  if (! request.url().includes('/sync/push')) {
    return;
  }

  const body = JSON.parse(request.postData() ?? '{}');

  if ((body.ops ?? []).length > 0) {
    pushes.push(body.ops.map((operation) => operation.op_id));
  }
});

await signIn(page);

// Two more windows on the same device, exactly as a service runs: library, control, stage.
const second = await context.newPage();
await second.goto(`${APP}/sets`);
const third = await context.newPage();
await third.goto(`${APP}/library`);
await third.waitForSelector('text=Amazing Grace', { timeout: 20000 });

// An edit made while offline, so all three windows have it in the outbox at the same moment.
await context.setOffline(true);
await page.bringToFront();
const title = `One worker ${Date.now()}`;
await page.getByPlaceholder('Add a song…').fill(title);
await page.getByRole('button', { name: 'Add', exact: true }).click();
await page.waitForSelector(`text=${title}`, { timeout: 10000 });
log('made a song offline:', title);

pushes.length = 0;
await context.setOffline(false);
await Promise.all([page, second, third].map((each) => each.evaluate(() => window.dispatchEvent(new Event('online')))));
await page.waitForTimeout(40000);

console.log('  outbox afterwards:', JSON.stringify(await page.evaluate(async () => {
  const name = (await indexedDB.databases()).map((d) => d.name ?? '').find((n) => n.startsWith('aurum-'));
  return await new Promise((resolve) => {
    const open = indexedDB.open(name);
    open.onsuccess = () => {
      const request = open.result.transaction('outbox', 'readonly').objectStore('outbox').getAll();
      request.onsuccess = () => { open.result.close(); resolve(request.result.map((o) => `${o.table}/${o.status}/${o.last_error ?? 'ok'}`)); };
    };
  });
})));

const ids = pushes.flat();
log('windows open: 3');
log('pushes carrying operations:', pushes.length, JSON.stringify(pushes));
if (new Set(ids).size !== ids.length) throw new Error('The same operation was pushed more than once');
if (pushes.length > 1) throw new Error(`The outbox was drained ${pushes.length} times`);

await browser.close();
console.log('\nThree windows, one sync worker: the outbox was drained once.');
