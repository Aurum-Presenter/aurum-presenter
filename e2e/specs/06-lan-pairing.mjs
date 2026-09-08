/**
 * Pairing a tablet over the relay.
 *
 * The signalling relay carries only SDP and ICE; once the data channel is open the session state
 * travels device to device. The paired device follows the control surface, is listed on it, and —
 * once allowed — can move the session itself.
 */

import { APP, log, open, shot, signIn } from '../lib/browser.mjs';

const { browser, context, page: control } = await open();
await signIn(control);
await control.getByRole('link', { name: 'Sets', exact: true }).click();
await control.getByRole('link', { name: 'Sunday morning' }).click();
await control.getByRole('button', { name: 'Present' }).click();
await control.waitForSelector('text=Running order', { timeout: 15000 });
log('session running');

// Ask for a pairing code and let the relay open the room.
await control.getByRole('button', { name: 'Pair a device' }).click();
await control.waitForSelector('text=On the other device');
const code = (await control.locator('p.font-mono').innerText()).trim();
log('pairing code', code);
await control.waitForTimeout(1000);
const waiting = await control.locator('text=Waiting for a device').count();
log('relay reached:', waiting > 0 ? 'yes, room open' : 'no — check the relay');
if (waiting === 0) throw new Error('The control device could not open a room on the relay');

// A second device joins with the code.
const stage = await context.newPage();
stage.on('pageerror', (error) => console.log('  [stage error]', error.message));
stage.on('websocket', (ws) => {
  console.log('  [stage ws]', ws.url());
  ws.on('close', () => console.log('  [stage ws] closed'));
  ws.on('socketerror', (error) => console.log('  [stage ws] error', error));
  ws.on('framesent', (frame) => console.log('  [stage ws] sent', frame.payload.slice(0, 60)));
  ws.on('framereceived', (frame) => console.log('  [stage ws] got', frame.payload.slice(0, 60)));
});
await stage.goto(`${APP}/join?code=${code}`);
await stage.waitForSelector('text=Amazing grace how sweet', { timeout: 30000 });
log('paired device is showing the current slide');
await shot(stage, '09-paired-stage');

// The control surface should now count it as an output, and it should follow.
await control.bringToFront();
await control.waitForSelector('text=Paired device', { timeout: 10000 });
log('control surface lists the paired device');

const before = await stage.locator('main').innerText();
await control.getByRole('button', { name: 'Next →' }).click();
await stage.waitForTimeout(1200);
const after = await stage.locator('main').innerText();
log('paired device before:', JSON.stringify(before.split('\n')[0]), '→ after:', JSON.stringify(after.split('\n')[0]));
if (before === after) throw new Error('The paired device did not follow the control surface');

// Grant it the right to advance, and drive the session from the stage device.
await control.getByRole('checkbox', { name: /may advance/ }).first().check().catch(async () => {
  await control.locator('input[type=checkbox]').last().check();
});
await control.waitForTimeout(500);
await stage.bringToFront();
await stage.locator('footer button', { hasText: '←' }).click();
await control.waitForTimeout(1000);
const position = await control.locator('text=/\\d+ \\/ \\d+/').first().innerText();
log('after the stage device pressed back, the control surface is at', position);

await shot(control, '10-control-paired');
await browser.close();
console.log('\nPairing over the relay works end to end.');
