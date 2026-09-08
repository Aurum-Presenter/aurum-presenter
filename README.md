# Aurum Presenter

Offline-first, installable web app for managing chord charts and sheet music, and for
presenting lyrics on a multi-screen stage setup.

The specification lives in [`docs/SDD/`](docs/SDD/index.md) and is written before any code.
Start with the [system overview](docs/SDD/overview.md), then the feature documents and change
requests listed in the [index](docs/SDD/index.md).

## Layout

```
crates/core  the domain rules            compiled for the server and for WebAssembly
crates/api   axum · rusqlite · SQLite    the API, the pairing relay and the console commands
crates/web   Leptos, compiled to wasm    the installable PWA (being built)
frontend/    React 19 · TypeScript       the PWA it is replacing
e2e/         twelve Playwright scripts   the acceptance gate for both
compose.yaml the stack: API, MinIO, Mailpit — and no database service
```

`crates/core` is why the workspace is shaped this way. The rules both halves have to agree on —
which columns sync, per-field last-writer-wins, object keys, UUIDv7, the timestamp format — are
written once and compiled twice: natively for the server, and to WebAssembly for the browser.
They cannot drift, because there is one implementation.

## The shape of the backend

There is **one SQLite file per workspace**, plus one `control.sqlite` holding accounts,
sessions, workspaces and memberships. Two consequences run through the whole codebase:

- **Content tables carry no `workspace_id`.** The file is the scope, so a query cannot forget
  the predicate — there is no predicate. This is what replaced Postgres row-level security.
- **A workspace connection can only be opened from a `Workspace<P>`**, which is what the
  extractor hands a handler after verifying membership. The permission is the type parameter —
  a handler that writes asks for `Workspace<Write>` — so authorization is in the signature
  rather than in a route option read reflectively, and a handler that runs is already
  authorized and must not re-check.

`change_seq` — the delta-pull watermark — comes from a single-row counter bumped inside
`BEGIN IMMEDIATE`, which serialises it exactly the way the per-workspace Postgres sequence did,
and without the gaps a sequence leaves on rollback.

## Running it

```bash
make up          # builds, starts the stack, applies migrations
make web         # Vite dev server on :5173
```

`make up` creates `.env` from `.env-dist` with freshly generated keys on first run. Nothing
else is required — there is no database container to provision and no cloud account to
configure.

| | |
|---|---|
| API | http://localhost:8080/api/v1/health |
| Stage pairing relay | ws://localhost:8081 |
| Mail (Mailpit) | http://localhost:8025 |
| Object storage console | http://localhost:9001 |
| PWA (dev) | http://localhost:5173 |

Other targets: `make migrate`, `make purge`, `make workspaces`, `make mail`, `make signal`,
`make logs`, `make shell`, `make test`, `make lint`, `make web-test`, `make e2e`,
`make differential`. Run `make` on its own for the full list.

One process and one binary: `aurum-api` serves the HTTP API, runs the stage-pairing relay on a
second port, and carries the console commands as subcommands. The relay holds WebSockets open,
which is why it used to need a process of its own; it carries SDP and ICE between two devices on
a LAN and nothing else, and session state never reaches it.

## Checking it

```bash
make test           # the rules, the API against a real database, and the files themselves
make differential   # the same inputs through the TypeScript and the Rust, diffed
make e2e            # twelve browser scripts against the running stack
```

## Status

Specification **approved**, and every document in the index is now implemented: accounts,
workspaces and invitations; the song library with local search and import; chord charts with
key-signature-aware transposition, capo and Nashville numbers; sheet PDFs with offline files and
annotations; sets, reader mode and a printable pack; live presentation with audience output,
stage view and LAN pairing; the sync engine with its conflict review and storage controls; and
the installable PWA.

Two deliberate departures from the documents, both recorded where the code makes them.
**Chart parsing stays on the main thread** at every length: the feature document sends charts
over 500 lines to the search worker, but 500 lines parse in under 2 ms here and 10,000 in about
24 ms, so posting the text across and cloning the model back would cost more than the parse and
put an empty frame in front of somebody who is reading. And **stage pairing always goes through
the relay**: the change request prefers mDNS or a direct LAN address first, and a browser can do
neither, so the relay carries the SDP and ICE — and nothing else — on every network, including
one with client isolation.

Two properties run through all of it. **Charts are parsed and transposed entirely on the
device** (`frontend/src/chart/`): the server stores the ChordPro text and replicates it, and
never re-letters a chord — which is what lets two members read the same chart in two different
keys, offline, from one byte-identical body. And **nothing waits for the network**: every read
and every write goes to IndexedDB first, and sync is a background reconciliation that can fail
without a single screen changing.
