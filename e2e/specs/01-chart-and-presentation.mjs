/**
 * The everyday path: sign in, read a chart, transpose it, open a set, present it.
 *
 * Covers the chart pipeline end to end — a preferred key that is personal, a set key override that
 * is the band's, reader mode — and then the presentation itself: an audience window that follows the
 * control surface, and blanking that leaves the stage lit.
 */

import { APP, log, open, shot, signIn } from '../lib/browser.mjs';

const { browser, context, page } = await open();
await signIn(page);
log('signed in; the library shows the song that was pushed through the API');
await shot(page, '01-library');

// 2. Open the song: the chart should render, transposed on demand.
await page.getByRole('link', { name: /Amazing Grace/ }).first().click();
await page.waitForSelector('button:has-text("Key of")');

// The preferred key is a synced, per-user preference, so a fresh browser may open the song in
// whatever key this account last chose. Read it rather than assuming it.
const readKey = async () => (await page.getByRole('button', { name: /Key of/ }).innerText()).replace('Key of ', '').split('\n')[0].trim();
const chordsNow = async () => (await page.locator('.font-mono >> nth=0').innerText()).trim();

log('opened in the key of', await readKey());

await page.getByRole('button', { name: /Key of/ }).click();
await page.getByRole('button', { name: 'G', exact: true }).click();
await page.waitForTimeout(400);
const chordsInG = await chordsNow();
log('chart in G:', JSON.stringify(chordsInG.slice(0, 40)));
if (! chordsInG.startsWith('G')) throw new Error(`Expected G chords, got ${chordsInG}`);

await page.getByRole('button', { name: 'Bb', exact: true }).click();
await page.waitForTimeout(400);
const chordsInBb = await chordsNow();
log('chart in Bb:', JSON.stringify(chordsInBb.slice(0, 40)));
await page.keyboard.press('Escape');
await shot(page, '02-song-transposed');

if (! chordsInBb.includes('Bb')) throw new Error('Transposition did not reach the screen');

// 3. The set, its key override, and reader mode.
await page.getByRole('link', { name: 'Sets', exact: true }).click();
await page.getByRole('link', { name: 'Sunday morning' }).click();
await page.waitForSelector('text=set key');
log('set item shows the band-wide key override');
await shot(page, '03-set');

await page.getByRole('link', { name: 'Read' }).click();
await page.waitForSelector('text=1 / 1');
const readerText = await page.locator('article').innerText();
log('reader mode:', JSON.stringify(readerText.split('\n').slice(0, 2).join(' / ')));
if (! readerText.includes('A')) throw new Error('Reader mode did not render the set key');
await shot(page, '04-reader');

// 4. Presentation: start a session and check the audience window follows.
await page.goBack();
await page.getByRole('button', { name: 'Present' }).click();
await page.waitForSelector('text=Running order', { timeout: 15000 });
log('session started; control surface is up');

const audience = await context.newPage();
const url = new URL(page.url());
const sessionId = url.pathname.split('/').pop();
const workspaceId = await page.evaluate(() => localStorage.getItem('aurum.workspace'));
await audience.goto(`${APP}/output/audience?session=${sessionId}&workspace=${workspaceId}`);
await audience.waitForTimeout(1500);

const firstSlide = await audience.locator('body').innerText();
log('audience shows:', JSON.stringify(firstSlide.split('\n')[0]));

await page.bringToFront();
await page.getByRole('button', { name: 'Next →' }).click();
await audience.waitForTimeout(800);
const secondSlide = await audience.locator('body').innerText();
log('after Next, audience shows:', JSON.stringify(secondSlide.split('\n')[0]));
if (firstSlide === secondSlide) throw new Error('The audience window did not follow the control surface');

await shot(page, '05-control');
await shot(audience, '06-audience');

// Blanking the audience must not blank the stage.
await page.getByRole('button', { name: 'black', exact: true }).click();
await audience.waitForTimeout(600);
const blanked = (await audience.locator('body').innerText()).trim();
log('audience while blanked:', JSON.stringify(blanked));
if (blanked !== '') throw new Error('Blanking left content on the audience screen');
await shot(audience, '07-audience-blanked');

const stage = await context.newPage();
await stage.goto(`${APP}/output/stage?session=${sessionId}&workspace=${workspaceId}`);
await stage.waitForTimeout(1500);
const stageText = await stage.locator('main').innerText();
log('stage while the audience is blanked:', JSON.stringify(stageText.split('\n').slice(0, 2).join(' / ')));
if (stageText.trim() === '') throw new Error('Blanking the audience also blanked the stage');
await shot(stage, '08-stage');

await browser.close();
console.log('\nEnd-to-end run finished without an error.');
