#!/usr/bin/env node
/**
 * Runs the same inputs through the TypeScript and the Rust and reports every divergence.
 *
 * This is the evidence the client-side port is a port. Unit tests on the Rust side prove it does
 * what its author believed the rules were; this proves it does what the code that shipped
 * actually does, over a corpus neither author chose.
 *
 * A third arm used to run the server rules through the real PHP SyncService. It went when the
 * PHP did; what it established, and what carries those rules now, is in the README.
 *
 * Usage: node differential/run.mjs [cases-per-rule] [seed]
 */
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const ROOT = resolve(import.meta.dirname, '..');
const COUNT = Number(process.argv[2] ?? 10_000);
const SEED = Number(process.argv[3] ?? 20260908);
const ESBUILD = join(ROOT, 'frontend/node_modules/.bin/esbuild');

const run = (command, args, input) =>
  execFileSync(command, args, { cwd: ROOT, input, maxBuffer: 1 << 30, encoding: 'utf8' });

const work = mkdtempSync(join(tmpdir(), 'aurum-diff-'));
const bundle = join(work, 'typescript.mjs');

process.stderr.write('bundling the TypeScript…\n');
run(ESBUILD, [
  'differential/typescript.mjs',
  '--bundle',
  '--format=esm',
  '--platform=node',
  '--log-level=warning',
  `--outfile=${bundle}`,
]);

process.stderr.write(`generating ${COUNT.toLocaleString()} cases per rule (seed ${SEED})…\n`);
const cases = JSON.parse(run('node', [bundle, 'generate', String(SEED), String(COUNT)]));
const casesFile = join(work, 'cases.json');
writeFileSync(casesFile, JSON.stringify(cases));

process.stderr.write(`running ${cases.length.toLocaleString()} cases through the TypeScript…\n`);
const typescript = JSON.parse(run('node', [bundle], JSON.stringify(cases)));

process.stderr.write('building the Rust example…\n');
run('cargo', ['build', '--quiet', '--release', '-p', 'aurum-core', '--example', 'differential']);

process.stderr.write('running the same cases through the Rust…\n');
const rust = JSON.parse(run(join(ROOT, 'target/release/examples/differential'), [], JSON.stringify(cases)));

/** Order-insensitive for objects, exact for everything else, and NaN-free. */
const canonical = (value) => {
  if (Array.isArray(value)) {
    return value.map(canonical);
  }

  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  }

  // A float that agrees to six places agrees: the two languages round the last bit differently.
  return typeof value === 'number' && !Number.isInteger(value)
    ? Math.round(value * 1e6) / 1e6
    : value;
};

const same = (a, b) => JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));

const totals = new Map();
const divergences = [];

cases.forEach((entry, index) => {
  const total = totals.get(entry.rule) ?? { checked: 0, skipped: 0, diverged: 0 };
  totals.set(entry.rule, total);

  const before = typescript[index];

  if (before?.skipped === true) {
    total.skipped++;
    return;
  }

  total.checked++;

  if (!same(before, rust[index])) {
    total.diverged++;

    if (divergences.length < 20) {
      divergences.push({ rule: entry.rule, input: entry.input, before, rust: rust[index] });
    }
  }
});

process.stderr.write('\n');

let failed = 0;

for (const [rule, total] of [...totals].sort()) {
  const state = total.diverged > 0 ? 'DIFFERS' : total.checked === 0 ? 'skipped' : 'same';
  failed += total.diverged;

  process.stdout.write(
    ` ${state.padEnd(8)} ${rule.padEnd(14)} ${String(total.checked).padStart(7)} checked` +
    `${total.diverged > 0 ? `, ${total.diverged} diverged` : ''}` +
    `${total.skipped > 0 ? `, ${total.skipped} not comparable` : ''}\n`,
  );
}

if (divergences.length > 0) {
  process.stdout.write(`\nFirst ${divergences.length}:\n\n`);

  for (const divergence of divergences) {
    process.stdout.write(`${divergence.rule}\n  input:      ${JSON.stringify(divergence.input).slice(0, 400)}\n`);
    process.stdout.write(`  before:     ${JSON.stringify(divergence.before).slice(0, 400)}\n`);
    process.stdout.write(`  rust:       ${JSON.stringify(divergence.rust).slice(0, 400)}\n\n`);
  }
}

const cased = cases.length.toLocaleString();
process.stdout.write(failed > 0 ? `\n${failed} of ${cased} diverged (cases in ${casesFile})\n` : `\nno divergence over ${cased} cases\n`);
process.exit(failed > 0 ? 1 : 0);
