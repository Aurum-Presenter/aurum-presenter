# The differential harness

Proof that the Rust port was a port. **Retired** — both of the implementations it compared
against are gone, and this file is what it established.

The unit tests in `crates/core` say the Rust does what its author believed the rules were. This
said it does what the code that shipped actually did, over a corpus nobody chose by hand.

## What it was

Three arms, run against the same generated cases and compared on the *observable outcome* — the
chords on the screen, the slide list, the sheet that was chosen — never on the decision either
implementation made along the way, because the two were allowed to arrive there differently.

| Arm | What it ran | Went with |
|---|---|---|
| `php.mjs` | the server-only rules through the real `SyncService`, over a real SQLite workspace built from the real migrations | `backend/`, at the end of phase 2 |
| `typescript.mjs` | the client's rules, bundled straight out of `frontend/src` | `frontend/`, at the cutover |
| `crates/core/examples/differential.rs` | the Rust | — |

The corpus was deterministic: the same seed gave the same cases, so a divergence found in CI
could be reproduced exactly.

## The last run

Before the TypeScript was deleted, at seed `20260908`:

```
 same     chart            10000 checked
 same     importer         10000 checked
 same     over_lyrics      10000 checked
 same     pins             10000 checked
 same     rank             30000 checked
 same     search           10000 checked
 same     selection        10000 checked
 same     slides           10000 checked
 same     time             10000 checked
 same     transpose        10000 checked

no divergence over 120,000 cases
```

The PHP arm agreed with the Rust over 10,000 generated cases per rule for `merge`, `sync_schema`
and `object_keys` before it went.

## What it found

Four real defects, all of which had shipped:

- `firstChordKey` in the importer read `E♯` as `E`, because it re-parsed the raw text with a
  regex that knew only ASCII accidentals. Fixed in the TypeScript.
- Search returned tied results in whichever order the terms happened to be scanned, so two
  identical searches could disagree. Fixed in the TypeScript.
- The Rust importer stopped at the first *bracket* rather than the first *chord*, so a chart
  opening `[N.C.]` lost its key.
- The Rust notation detector allowed twelve characters inside a bracket where the TypeScript
  allowed thirteen, so `[Bbbmaj9#11/Db]` was not recognised as ChordPro.

The last two are why the harness existed. Neither would have been caught by a test written from
the same understanding that produced the bug.

## What holds these rules now

`crates/core` is the only implementation, so there is nothing left to diverge from. Its own
tests hold the rules; `crates/api/tests` drives the server-only ones against a real router over a
real database — a tombstone beating a stale edit, a displaced value kept for review, one default
arrangement per song, a viewer's limits, and no synced table letting a client write a
server-owned column; and `e2e/` drives the whole system through a browser.

`crates/core/examples/differential.rs` is kept. It is the Rust arm, and it still runs: a future
port — a second client, a native app — has a corpus and an oracle waiting for it.
