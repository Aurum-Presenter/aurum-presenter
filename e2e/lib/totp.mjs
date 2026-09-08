import { createHmac } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { ROOT } from './env.mjs';

/**
 * The authenticator's side of the two-factor exchange: a code from the same secret the account
 * was enrolled with.
 *
 * Thirty lines here rather than a dependency, because a test suite that has to be trusted as the
 * oracle for a rewrite should not import an implementation of the thing it is checking.
 */
export function totp(secret, at = Date.now()) {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
  let bits = '';

  for (const character of secret.replace(/=+$/, '').toUpperCase()) {
    bits += alphabet.indexOf(character).toString(2).padStart(5, '0');
  }

  const key = Buffer.from((bits.match(/.{8}/g) ?? []).map((byte) => parseInt(byte, 2)));
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(at / 1000 / 30)));

  const digest = createHmac('sha1', key).update(counter).digest();
  const offset = digest[digest.length - 1] & 0x0f;

  return String((digest.readUInt32BE(offset) & 0x7fffffff) % 1e6).padStart(6, '0');
}

/** Milliseconds until the next thirty-second window opens, plus a second of slack. */
export function untilNextWindow(at = Date.now()) {
  return 31_000 - (at % 30_000);
}

/**
 * A code nobody has used yet.
 *
 * A code is single-use inside its thirty-second window — correct of the app, and a trap for a
 * suite whose specs run back to back in separate processes: the second sign-in of a window is
 * refused. The window last used is recorded in a file so the next caller, whichever process it
 * is in, waits for a fresh one instead of being turned away.
 */
const LEDGER = join(ROOT, '.last-code');

export async function freshCode(secret) {
  const used = read();

  if (window(Date.now()) === used) {
    await new Promise((resolve) => setTimeout(resolve, untilNextWindow()));
  }

  const at = Date.now();
  writeFileSync(LEDGER, String(window(at)));

  return totp(secret, at);
}

function window(at) {
  return Math.floor(at / 30_000);
}

function read() {
  try {
    return existsSync(LEDGER) ? Number(readFileSync(LEDGER, 'utf8')) : -1;
  } catch {
    return -1;
  }
}
