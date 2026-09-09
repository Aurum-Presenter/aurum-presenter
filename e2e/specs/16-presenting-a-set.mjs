/**
 * Presenting a set: the control surface and the audience screen.
 *
 * The two things this proves are the two the whole feature rests on. The audience window is a
 * subscriber and nothing else — it renders the state it is handed, so advancing, blanking and a
 * message all reach it without it knowing anything about sets or songs. And it is counted: the
 * control surface knows the screen is there and which revision it has acknowledged, which is
 * what lets an operator see that the projector is actually following.
 *
 * Like specs 13 to 15 this needs no server: a service that has lost its wifi still has to run.
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
  await page.locator('textarea').first()
    .fill(`{verse: 1}\n[${key}]Line one of ${title}\nLine two of ${title}`);
  await page.waitForSelector('text=Saved', { timeout: 5000 });
  await page.getByRole('link', { name: '← Library' }).click();
  await page.waitForSelector('input[placeholder="Add a song…"]');
}

await page.getByRole('link', { name: 'Sets', exact: true }).click();
await page.getByPlaceholder(/New set/).fill('Sunday morning');
await page.getByRole('button', { name: 'Create', exact: true }).click();
await page.waitForSelector('text=Add items', { timeout: 15000 });
await page.getByRole('button', { name: 'Add items' }).click();

for (const [title] of SONGS) {
  await page.locator('li > button').filter({ hasText: title }).first().click();
}

await page.getByRole('button', { name: /^Add 2 songs$/ }).click();
await page.waitForSelector('ol > li', { timeout: 10000 });
log('a set with two songs');

await page.getByRole('button', { name: 'Present', exact: true }).click();
await page.waitForSelector('text=Running order', { timeout: 20000 });
log('the control surface is up');

// The audience window, opened by the control surface, in this same browser context.
const opening = context.waitForEvent('page');
await page.getByRole('button', { name: 'Audience window' }).click();
const audience = await opening;

const audienceText = () => audience.evaluate(() => document.body.innerText.trim());

await audience.waitForFunction(
  () => document.body.innerText.includes('Line one of Cornerstone'),
  { timeout: 20000 },
);
log('the audience shows the first slide');
await shot(page, '16-control');

// The control surface counts it, and knows which revision it has acknowledged.
await page.waitForFunction(
  () => !document.body.innerText.includes('No screens attached yet'),
  { timeout: 20000 },
);
log('the control surface counts the screen');

if (! (await page.locator('body').innerText()).includes('revision')) {
  throw new Error('The audience window is attached but is not acknowledging anything');
}

// Advancing moves the audience without the audience knowing what a set is. Named exactly: a
// looser check passes on the slide that was already up, which proves nothing.
await page.getByRole('button', { name: 'Next →' }).click();
await audience.waitForFunction(
  () => document.body.innerText.includes('Line one of Amazing Grace'),
  { timeout: 15000 },
);
log('advancing moved the audience to:', JSON.stringify((await audienceText()).slice(0, 40)));

// Blanking clears the room and nothing else.
await page.getByRole('button', { name: 'black', exact: true }).click();
await audience.waitForFunction(() => document.body.innerText.trim() === '', { timeout: 15000 });
log('blanked');
await shot(audience, '16-blanked');

await page.getByRole('button', { name: 'black', exact: true }).click();
await audience.waitForFunction(() => document.body.innerText.trim() !== '', { timeout: 15000 });
log('and unblanked');

// A message for the room.
await page.getByPlaceholder(/Message on the audience screen/).fill('Beginning in five minutes');
await page.getByRole('button', { name: 'Show', exact: true }).click();
await audience.waitForFunction(
  () => document.body.innerText.includes('Beginning in five minutes'),
  { timeout: 15000 },
);
log('a message reached the room');

// Ending puts the operator back on the set.
await page.getByRole('button', { name: 'End', exact: true }).click();
await page.getByRole('button', { name: 'End session' }).click();
await page.waitForSelector('text=Add items', { timeout: 20000 });
log('the session ended and the operator is back on the set');

log('OK');

await context.close();
await browser.close();
