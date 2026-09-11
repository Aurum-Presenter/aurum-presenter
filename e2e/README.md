# End-to-end verification

Twelve scripts that drive the real stack through a real browser and assert what a musician would
see. Between them they cover every acceptance criterion in `docs/SDD` that a machine can judge:
transposition and set overrides, presentation and pairing, sheets and annotations, offline start
and offline writes, conflicts, local-only mode and claiming, storage pressure, session end.

They are deliberately black-box. Nothing here imports application code, and nothing knows which
language either half is written in — which is what made this suite the oracle for the Rust
rewrite: the same scripts, unweakened, passed against the PHP API and the React client, then
against the Rust API with that same React client, and then against the Leptos client.

## Running

The stack must be up: `make up` is enough, because the API binary serves the client's assets,
runs the signalling relay on a second port, and the object store comes up beside it.

```bash
make e2e                       # seed a world, then run everything
make e2e SPEC=sheets           # only specs whose name contains "sheets"
node run.mjs --no-seed         # reuse the world from the last run
node lib/seed.mjs              # seed only
```

Point it somewhere else with `AURUM_APP` and `AURUM_API` — `AURUM_APP=http://127.0.0.1:4174`
runs it against Trunk's dev server, which is what a client change is worked against.
`CHROME_PATH` selects the browser.
Screenshots land in `shots/` (or `SHOTS`).

## How a run gets its world

`lib/seed.mjs` registers a **fresh account on every run** — a timestamped address, two-factor
enrolled, a song with a chart, a set dated inside the auto-pin window, and a sheet whose bytes are
really in the object store. It writes `.state.json`, which the specs read.

Nothing depends on a database somebody prepared by hand, so the suite runs against an empty stack
and two runs cannot tread on each other.

## What the specs may assume

Only what the app promises. Three of them do reach into the device's own storage — a mark that
must survive a file being replaced, a pin that must stop being wanted, a workspace that must come
off the device entirely — and those reads go through `lib/local.mjs` rather than being spelled out
inline. The names they use are part of the contract:

- the local database is `aurum-<workspace id>`, one store per synced table;
- `localStorage` keys are `aurum.workspace`, `aurum.local`, `aurum.session.active`,
  `aurum.stage.last`, `aurum.released.<workspace id>`;
- cached sheet files live in the origin private file system under `aurum-<workspace id>`.

A rewrite that keeps those names keeps this suite as its safety net.

## Layout

```
run.mjs        seeds, runs each spec in its own process, prints the summary
lib/env.mjs    where the stack is, where the fixtures are, what the last seed left
lib/api.mjs    a small API client — for provisioning only, never for asserting
lib/seed.mjs   builds the world
lib/browser.mjs  launching, signing in (including the two-factor dance), screenshots
lib/totp.mjs   the authenticator's side of two-factor
lib/local.mjs  reading IndexedDB and the file store from inside the page
specs/         the twelve scripts, run in name order
fixtures/      a one-page PDF, a two-page PDF, a background image
```

## Writing another one

Keep them in the same voice: a doc comment saying what the script is really checking and why it
matters, `log()` lines a person can read as the run goes by, and a closing sentence that states
the promise the run has just proved. A failure should throw with a sentence, not an assertion
code.
