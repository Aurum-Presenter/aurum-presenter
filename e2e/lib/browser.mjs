import { chromium } from 'playwright';
import { APP, CHROME, log, shotsDir, state } from './env.mjs';
import { freshCode, untilNextWindow } from './totp.mjs';

/**
 * Driving the app the way a person does.
 *
 * Every spec goes through here, so the sign-in dance — password, then a two-factor code that is
 * refused if it has already been used inside its window — is written once instead of twelve
 * times.
 */
export async function open({ viewport = { width: 1280, height: 860 }, quiet = false } = {}) {
  const browser = await chromium.launch({ executablePath: CHROME });
  const context = await browser.newContext({ viewport });
  const page = await context.newPage();

  if (! quiet) {
    watch(page);
  }

  return { browser, context, page };
}

/** Surfaces the failures a headless run would otherwise swallow. */
export function watch(page, label = '') {
  const prefix = label === '' ? ' ' : ` [${label}]`;

  page.on('pageerror', (error) => console.log(` ${prefix}page error:`, error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') console.log(` ${prefix}console error:`, message.text().slice(0, 200));
  });
  page.on('response', (response) => {
    if (response.status() >= 400 && ! response.url().includes('auth/refresh')) {
      console.log(` ${prefix}http ${response.status()}`, response.url().slice(-70));
    }
  });
}

export async function signIn(page, account = state().account, { landmark = 'Amazing Grace' } = {}) {
  await page.goto(`${APP}/library`);
  await page.getByPlaceholder('Email').fill(account.email);
  await page.getByPlaceholder('Password').fill(account.password);
  await page.getByRole('button', { name: 'Continue' }).click();
  await page.waitForTimeout(1500);

  if (await page.getByPlaceholder('000000').count() > 0) {
    await enterCode(page, account.secret);
  }

  if (landmark !== null) {
    await page.waitForSelector(`text=${landmark}`, { timeout: 20_000 });
  }
}

/**
 * A code already used inside its thirty-second window is refused — the app behaving correctly,
 * and exactly what happens when specs run back to back. Wait for the next window and try again.
 */
export async function enterCode(target, secret) {
  for (let attempt = 0; attempt < 3; attempt++) {
    await target.getByPlaceholder('000000').fill(await freshCode(secret));
    await target.getByRole('button', { name: 'Continue' }).click();
    await target.waitForTimeout(1500);

    if (await target.getByPlaceholder('000000').count() === 0) {
      return;
    }

    await target.waitForTimeout(untilNextWindow());
  }

  throw new Error('The two-factor prompt would not accept a fresh code.');
}

export async function shot(page, name) {
  await page.screenshot({ path: `${shotsDir()}/${name}.png` });
}

export { APP, log };
