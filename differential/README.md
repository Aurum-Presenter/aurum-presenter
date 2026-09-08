# The differential harness

Proof that the Rust port is a port.

The unit tests in `crates/core` say the Rust does what its author believed the rules were. This
says it does what the code that shipped actually does — over a corpus nobody chose by hand, run
through all three implementations at once:

| Side | What it runs |
|---|---|
| `typescript.mjs` | the client's rules, bundled straight out of `frontend/src` |
| `php.php` | the server's rules, driven through the real `SyncService` over a real SQLite workspace built from the real migrations |
| `crates/core/examples/differential.rs` | the Rust |

Each rule is checked against whichever of the two it was ported from; the rules that only ever
existed on the server (`merge`, `sync_schema`, `object_keys`) go to the PHP, the rest to the
TypeScript. What is compared is the *observable outcome* — the row a user would see afterwards,
the chords on the screen — not the decision either implementation made along the way, because
the two are allowed to arrive there differently.

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

- Structured payload fields in `merge` are generated as scalars, because the client already
  JSON-encodes its list fields to strings before pushing them. Comparing two languages'
  `json_encode` would be comparing escaping conventions, not rules.
- Column defaults: a seeded row carries every column of its table, so no comparison lands on a
  value neither implementation wrote.
- Constraint violations are the database's job, not this crate's. The generator respects the
  schema so that every case reaches a rule rather than dying at the door.
