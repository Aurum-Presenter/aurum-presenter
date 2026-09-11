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
crates/web   Leptos, compiled to wasm    the installable PWA
web/         Tailwind · Workbox · Pdfium the client's non-Rust assets
e2e/         twelve Playwright scripts   the acceptance gate
compose.yaml the stack: API, MinIO, Mailpit — and no database service
```

`crates/core` is why the workspace is shaped this way. The rules both halves have to agree on —
which columns sync, per-field last-writer-wins, object keys, UUIDv7, the timestamp format — are
written once and compiled twice: natively for the server, and to WebAssembly for the browser.
They cannot drift, because there is one implementation.

Three things are not Rust, and none of them is application logic. Tailwind reads the Rust for the
class names to emit. Workbox writes the service worker over the distribution Trunk has just
produced, because precaching hashed filenames means knowing them. And Pdfium — Chrome's PDF
engine, which renders the sheet music — arrives with Emscripten loader glue that the app loads
and never calls into.

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
make up          # builds both halves, starts the stack, applies migrations
```

`make up` creates `.env` from `.env-dist` with freshly generated keys on first run. Nothing
else is required — there is no database container to provision and no cloud account to
configure.

| | |
|---|---|
| The app, and the API under `/api/v1` | http://localhost:8080 |
| Stage pairing relay | ws://localhost:8081 |
| Mail (Mailpit) | http://localhost:8025 |
| Object storage console | http://localhost:9001 |

To work on the client, run its own dev server instead — it rebuilds on save and proxies the API
back to the container:

```bash
make web         # Trunk on :4174
```

Other targets: `make web-build`, `make migrate`, `make purge`, `make workspaces`, `make mail`,
`make signal`, `make logs`, `make shell`, `make test`, `make lint`, `make e2e`. Run `make` on its
own for the full list.

**One process and one binary.** `aurum-api` serves the HTTP API, serves the client's built
assets, runs the stage-pairing relay on a second port, and carries the console commands as
subcommands. The relay holds WebSockets open, which is why it used to need a process of its own;
it carries SDP and ICE between two devices on a LAN and nothing else, and session state never
reaches it.

Serving the app from the API is also what makes every client-side route survive a reload: an
unknown path is answered with the shell rather than a 404, while an unknown path under `/api/v1`
is still a JSON error — the sync engine classifies failures by status and parses the body.

## Checking it

```bash
make test           # the rules, the API against a real database, and the files themselves
make e2e            # twelve browser scripts against the running stack
make lint           # clippy with warnings denied, and rustfmt
```

## Status

Specification **approved**, and every document in the index is implemented: accounts, workspaces
and invitations; the song library with local search and import; chord charts with
key-signature-aware transposition, capo and Nashville numbers; sheet PDFs with offline files and
annotations; sets, reader mode and a printable pack; live presentation with audience output,
stage view and LAN pairing; the sync engine with its conflict review and storage controls; and
the installable PWA.

The rewrite recorded in [the change request](docs/SDD/change-request/2026-09-08-rust-rewrite.md)
is complete: the PHP and the TypeScript are both gone, and one workspace holds both halves.

Three deliberate departures from the documents, each recorded where the code makes it.
**Chart parsing stays on the main thread** at every length: the feature document sends charts
over 500 lines to the search worker, but 500 lines parse in under 2 ms here and 10,000 in about
24 ms, so posting the text across and cloning the model back would cost more than the parse and
put an empty frame in front of somebody who is reading. **Stage pairing always goes through the
relay**: the change request prefers mDNS or a direct LAN address first, and a browser can do
neither, so the relay carries the SDP and ICE — and nothing else — on every network, including
one with client isolation. And **the output windows have no error boundary**, because a Rust
render cannot throw; what the React one protected against is structural now, since the audience
and stage screens paint the theme background before any state arrives.

Two properties run through all of it. **Charts are parsed and transposed entirely on the device**
(`crates/core/src/chart/`): the server stores the ChordPro text and replicates it, and never
re-letters a chord — which is what lets two members read the same chart in two different keys,
offline, from one byte-identical body. And **nothing waits for the network**: every read and
every write goes to IndexedDB first, and sync is a background reconciliation that can fail
without a single screen changing.
