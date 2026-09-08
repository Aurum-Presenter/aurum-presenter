# The differential harness

Proof that the Rust port is a port.

The unit tests in `crates/core` say the Rust does what its author believed the rules were. This
says it does what the code that shipped actually does — over a corpus nobody chose by hand, run
through all three implementations at once:

| Side | What it runs |
|---|---|
| `typescript.mjs` | the client's rules, bundled straight out of `frontend/src` |
| `crates/core/examples/differential.rs` | the Rust |

What is compared is the *observable outcome* — the chords on the screen, the slide list, the
sheet that was chosen — not the decision either implementation made along the way, because the
two are allowed to arrive there differently.

A third arm used to run the server-only rules (`merge`, `sync_schema`, `object_keys`) through
the real `SyncService`, over a real SQLite workspace built from the real migrations. It agreed
with the Rust over 10,000 generated cases per rule, and it went when the PHP did. Those rules
are now held by `crates/core`'s own tests and by the integration tests in `crates/api/tests`,
which drive the same outcomes against a real server: a tombstone beating a stale edit, a
displaced value kept for review, one default arrangement per song, a viewer's limits, and no
synced table letting a client write a server-owned column.

## Running it

```
make differential            # 10,000 cases per rule
node differential/run.mjs 500 12345   # fewer cases, a different seed
```

The corpus is deterministic: the same seed gives the same cases, so a divergence found in CI can
be reproduced exactly. A failing run prints the first twenty and leaves the whole corpus in a
temporary file.

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

## What it does not cover

Anything with I/O. These are the pure rules; the API's behaviour is covered by
`crates/api/tests`, and the whole system by `e2e/`.
