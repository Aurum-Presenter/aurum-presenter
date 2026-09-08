import { readFileSync, writeFileSync } from 'node:fs';
import { Api, uuidv7 } from './api.mjs';
import { fixture, log, STATE_FILE } from './env.mjs';
import { freshCode, untilNextWindow } from './totp.mjs';

/**
 * Builds the world the specs read: an account with two-factor enabled, a song with a chart, a set
 * dated inside the auto-pin window, and a sheet whose bytes are really in the object store.
 *
 * A fresh account every run, with a timestamped address. Runs cannot then interfere with each
 * other, nothing depends on a database somebody set up by hand months ago, and the suite works
 * against an empty stack — which is exactly the state a rewrite starts from.
 */
const PASSWORD = 'a-long-enough-password';

const CHART = `{verse: 1}
[G]Amazing grace how [C]sweet the [G]sound
that [G]saved a wretch like [D]me

{chorus}
[C]My chains are [G]gone, I've been set [D]free
my [G]God, my [C]Saviour has ransomed [G]me

{verse: 2}
[G]'Twas grace that [C]taught my [G]heart to fear
and [G]grace my fears re[D]lieved
`;

export async function seed() {
  const api = new Api();
  const stamp = Date.now();
  const email = `e2e-${stamp}@aurum.test`;

  await api.register(email, 'Kate', PASSWORD);

  const account = await api.me();
  const workspace = account.workspaces[0].id;

  const { secret } = await api.enrolTotp();
  await api.confirmTotp(await freshCode(secret));

  const song = uuidv7();
  const set = uuidv7();
  const sheet = uuidv7();

  const scheduled = new Date(stamp + 5 * 86_400_000).toISOString().slice(0, 10);

  await api.push(workspace, [
    op('songs', song, {
      title: 'Amazing Grace',
      artist: 'John Newton',
      original_key: 'G',
      tempo: 72,
      tags: '["hymn"]',
    }),
    op('arrangements', uuidv7(), {
      song_id: song,
      name: 'Default',
      is_default: 1,
      default_key: 'G',
      source_notation: 'chordpro',
      body: CHART,
    }),
    op('sets', set, { name: 'Sunday morning', scheduled_for: scheduled, venue: 'Main hall', pinned: 0 }),
    op('set_items', uuidv7(), {
      set_id: set,
      rank: 'V',
      song_id: song,
      title_snapshot: 'Amazing Grace',
      key_override: 'A',
    }),
    op('sheets', sheet, {
      song_id: song,
      sheet_key: 'G',
      part: 'piano',
      filename: 'piano.pdf',
      mime_type: 'application/pdf',
      position: 0,
    }),
  ]);

  const bytes = readFileSync(fixture('piano.pdf'));
  await api.uploadSheet(workspace, sheet, bytes);

  const world = {
    seeded_at: new Date(stamp).toISOString(),
    account: { email, password: PASSWORD, secret },
    workspace,
    song,
    set,
    sheet,
  };

  writeFileSync(STATE_FILE, `${JSON.stringify(world, null, 2)}\n`);

  return world;
}

function op(table, recordId, payload) {
  return { op_id: uuidv7(), table, record_id: recordId, op: 'upsert', payload };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  // A code is single-use inside its window, and the sign-in that follows a seed will need one of
  // its own. Handing the next window to the specs costs a second and saves a retry.
  const world = await seed();

  log(`seeded ${world.account.email}`);
  log(`workspace ${world.workspace}`);

  if (untilNextWindow() < 3_000) {
    await new Promise((resolve) => setTimeout(resolve, untilNextWindow()));
  }
}
