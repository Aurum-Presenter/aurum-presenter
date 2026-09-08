import { spawn } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import { API, APP, ROOT } from './lib/env.mjs';
import { seed } from './lib/seed.mjs';

/**
 * Runs the suite: seed a world, then each spec in its own process, in order.
 *
 * One process per spec because they are long, stateful browser sessions — a crash in one must not
 * take the rest with it, and the summary at the end is what says whether the app is shippable.
 *
 *   node run.mjs                  everything
 *   node run.mjs sheets theme     only specs whose name contains one of these
 *   node run.mjs --no-seed        reuse the world from the last run
 */
const args = process.argv.slice(2);
const reseed = ! args.includes('--no-seed');
const filters = args.filter((arg) => ! arg.startsWith('--'));

const specs = readdirSync(join(ROOT, 'specs'))
  .filter((name) => name.endsWith('.mjs'))
  .sort()
  .filter((name) => filters.length === 0 || filters.some((filter) => name.includes(filter)));

if (specs.length === 0) {
  console.error('No specs matched.');
  process.exit(1);
}

console.log(`app ${APP}  api ${API}`);

if (! await reachable(`${API}/api/v1/health`) || ! await reachable(APP)) {
  console.error('The stack is not up. Start the API, the object store and the web server first.');
  process.exit(1);
}

if (reseed) {
  const world = await seed();
  console.log(`seeded ${world.account.email} in workspace ${world.workspace}\n`);
}

const results = [];

for (const spec of specs) {
  process.stdout.write(`── ${spec}\n`);

  const started = Date.now();
  const code = await run(join(ROOT, 'specs', spec));
  const seconds = ((Date.now() - started) / 1000).toFixed(0);

  results.push({ spec, ok: code === 0, seconds });
  process.stdout.write(code === 0 ? `   passed in ${seconds}s\n\n` : `   FAILED in ${seconds}s\n\n`);
}

const failed = results.filter((result) => ! result.ok);

console.log('─'.repeat(60));

for (const result of results) {
  console.log(`${result.ok ? ' ok  ' : 'FAIL '} ${result.spec.padEnd(38)} ${result.seconds}s`);
}

console.log(`\n${results.length - failed.length}/${results.length} passed`);
process.exit(failed.length === 0 ? 0 : 1);

function run(path) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [path], { stdio: 'inherit', cwd: ROOT });
    const timer = setTimeout(() => child.kill('SIGKILL'), 300_000);

    child.on('exit', (code) => {
      clearTimeout(timer);
      resolve(code ?? 1);
    });
  });
}

async function reachable(url) {
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(5_000) });

    return response.status < 500;
  } catch {
    return false;
  }
}
