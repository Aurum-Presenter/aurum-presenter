/**
 * Using the app with no account, then signing up and keeping everything.
 *
 * The workspace id is generated on the device, so the claim hands the server the id the local
 * records already carry — nothing is re-created and nothing is re-downloaded.
 */

import { APP, log, open, shot } from '../lib/browser.mjs';

// This spec makes its own account: the point of it is the path from no account to a claimed one,
// so the seeded world is deliberately not used.
const { browser, page } = await open({ viewport: { width: 1280, height: 800 } });

// 1. Use the app with no account at all.
await page.goto(`${APP}/library`);
await page.getByRole('button', { name: 'use it on this device without an account' }).click();
await page.waitForSelector('text=This device is working on its own', { timeout: 10000 });
log('running with no account');

await page.getByPlaceholder('Add a song…').fill('Song written before signing up');
await page.getByRole('button', { name: 'Add', exact: true }).click();
await page.waitForSelector('text=What key is this chart written in?', { timeout: 10000 });
await page.getByRole('button', { name: 'D', exact: true }).click();
await page.getByRole('button', { name: 'Add a chart' }).click();
await page.locator('textarea').fill('{verse: 1}\n[D]Made on a device with [A]no account');
await page.waitForTimeout(1200);
log('a song and a chart exist locally');

const workspaceId = await page.evaluate(() => JSON.parse(localStorage.getItem('aurum.local') ?? '{}').workspaceId);
log('local workspace id', workspaceId);

await shot(page, '16-local-mode');

// 2. Sign in, and the local workspace should be claimed under the same id.
await page.getByRole('button', { name: 'Sign in and keep it' }).click();
await page.getByRole('button', { name: 'Go to sign in' }).click();
await page.waitForSelector('input[placeholder="Email"]', { timeout: 10000 });
await page.getByRole('button', { name: 'Create an account' }).click();

// A fresh address each run, so a re-run is not blocked by the account the last one made.
const email = `claimer-${Date.now()}@example.com`;
await page.getByPlaceholder('Email').fill(email);
await page.getByPlaceholder('Your name').fill('Jo');
await page.getByPlaceholder('Password').fill('a-long-enough-password');
await page.getByRole('button', { name: 'Continue' }).click();

await page.waitForTimeout(6000);
console.log('  after sign-up the page says:', JSON.stringify((await page.locator('body').innerText()).slice(0, 220).replace(/\n/g, ' | ')));
await page.waitForSelector('text=Song written before signing up', { timeout: 20000 });
log('after signing up, the song made offline is still there');

const workspaces = await page.locator('header select').first().innerText();
log('workspaces now:', JSON.stringify(workspaces.replace(/\n/g, ' | ')));

const stillLocal = await page.evaluate(() => localStorage.getItem('aurum.local'));
log('local mode cleared:', stillLocal === null);
if (stillLocal !== null) throw new Error('The local workspace was not claimed');

// The claimed workspace keeps its id, so nothing had to be re-created.
const options = await page.locator('header select option').evaluateAll((nodes) => nodes.map((n) => n.value));
log('claimed the same workspace id:', options.includes(workspaceId));
if (! options.includes(workspaceId)) throw new Error('The workspace was claimed under a different id');

await page.getByTitle(/Nothing is blocked/).click();
await page.getByRole('button', { name: 'Sync now' }).click();
await page.waitForTimeout(2500);
const pending = await page.locator('dd').nth(1).innerText();
log('everything made offline has been pushed:', JSON.stringify(pending));
await shot(page, '17-claimed');

await browser.close();
console.log('\nLocal-only mode and claiming work.');
