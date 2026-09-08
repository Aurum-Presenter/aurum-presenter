---
id: rust-rewrite
title: One language for both halves — Rust on the server, Rust and WebAssembly in the client
type: change-request
status: Approved
created: 2026-09-08
updated: 2026-09-08
changes:
  - feature/2026-09-06-workspaces-and-access.md
  - feature/2026-09-06-song-library.md
  - feature/2026-09-06-chord-charts-and-transposition.md
  - feature/2026-09-06-sheet-attachments.md
  - feature/2026-09-06-sets.md
  - feature/2026-09-06-offline-storage-and-sync.md
  - feature/2026-09-06-pwa-installation.md
  - feature/2026-09-06-presentation.md
  - feature/2026-09-06-presenter-output.md
  - feature/2026-09-06-stage-view.md
related:
  - change-request/2026-09-06-sqlite-backend.md
  - change-request/2026-09-06-self-hosted-auth-totp.md
  - change-request/2026-09-06-s3-sheet-storage.md
  - change-request/2026-09-06-lan-websocket-signalling.md
---

# One language for both halves — Rust on the server, Rust and WebAssembly in the client

## Context

The specification was written once; the code implements it twice. A PHP API of 7,173 lines and a
TypeScript client of 13,524 lines are not two independent programs — they are two implementations
of the same rules, and a set of those rules has to agree exactly for the app to work at all:

- which columns of which tables a client may write (`SyncSchema` on the server, the Dexie schema
  and the repositories on the client);
- per-field last-writer-wins, tombstone precedence, and what counts as a conflict;
- how an object key is derived from a workspace and a content hash;
- UUIDv7, and the millisecond ISO-8601 format that makes SQLite's string comparison chronological;
- the `change_seq` watermark protocol.

Nothing enforces that agreement. It has held so far because one person wrote both sides in one
sitting; it is exactly the kind of thing that drifts the first time somebody fixes a bug on one
side of the wire. Everything else in this system has been arranged so that a rule cannot be
forgotten — the workspace *is* the file, so a query cannot omit the predicate — and this is the one
place left where correctness rests on discipline.

Rust removes it. One crate of domain rules compiles natively for the server and to WebAssembly for
the browser, so the two halves cannot disagree: there is one implementation.

## Current behaviour

**Server** — PHP 8.4, Mezzio 3 (PSR-15), attribute-declared routes discovered reflectively at
build time, a service-manager container with seventeen factories, Doctrine DBAL over `pdo_sqlite`,
the AWS SDK for object storage, `otphp` for TOTP, Symfony Mailer, `laminas-cli` for the four
console commands, and a hand-written RFC 6455 WebSocket relay — 669 lines of framing, masking and
`stream_select` — for stage pairing.

**Client** — React 19 with `react-router-dom`, Dexie over IndexedDB with `liveQuery` driving
reactivity, OPFS for sheet files, `pdf.js` for rendering them, Workbox and `vite-plugin-pwa` for
the installable shell, Tailwind, Vite, Vitest.

## Requested change

### One workspace, three crates

```
crates/core   the domain rules — no I/O, no DOM; compiles for the host and for wasm32
crates/api    axum server and the console commands
crates/web    the Leptos client, compiled to WebAssembly
```

`core` holds everything both halves must agree on, and everything the client computes on its own:
notes and enharmonic spelling, transposition, ChordPro parsing and rendering, slide building,
sheet selection, fractional ranks, the search index, the pin policy, the import parser, the synced
schema, the merge rules, object keys, UUIDv7 and the timestamp format.

### Server

| Today | Becomes |
|---|---|
| Mezzio, laminas-di, laminas-router, diactoros, tempest/discovery | `axum` and `tower-http`: one explicit router, no container, no reflection |
| Doctrine DBAL, `pdo_sqlite` | `rusqlite`, calls on a blocking pool; the same pragmas and the same `BEGIN IMMEDIATE` batch |
| `aws/aws-sdk-php` | `aws-sdk-s3` |
| `spomky-labs/otphp` | `totp-rs` |
| Symfony Mailer | `lettre` |
| `laminas-cli` | `clap` |
| `password_hash` | `argon2` (argon2id) |
| hand-written RFC 6455 relay | `tokio-tungstenite` |

The HTTP contract does not change: the same thirty endpoints, the same JSON, the same status codes
and error codes. The migration `.sql` files are carried over unedited.

### Client

| Today | Becomes |
|---|---|
| React, `react-router-dom` | `leptos` (client-rendered) and `leptos_router` |
| Dexie, `dexie-react-hooks` | a typed IndexedDB layer over `web-sys`, with an observer that turns a write into a signal — the equivalent of `liveQuery` |
| `pdf.js` | `pdfium-render` — Chrome's PDF engine as WebAssembly — behind a `SheetRenderer` interface |
| Vite | Trunk |
| Vitest | `cargo test` for the rules, `wasm-bindgen-test` for the browser-bound layers |

Rendering stays client-side: no server-side rendering and no hydration, because the app must start
from a precached shell with the radio off.

**What stays JavaScript, and why.** Two things, neither of them application logic. The service
worker remains TypeScript with Workbox, because the precache manifest and the update-hold
behaviour are already exactly right and WebAssembly buys nothing inside a worker whose job is to
answer `fetch` events. And Pdfium arrives with Emscripten loader glue, which the app loads but
never calls into. Every rule, every screen and every byte of state handling is Rust.

### The sheet renderer

`pdfium-render` replaces `pdf.js` behind a narrow interface — page count, and render a page to a
bitmap — used by the viewer, the print pack and sheet slides alike.

Measured before committing to it, rendering the piano fixture in a headless browser:

| | Pdfium | `pdf.js` today |
|---|---|---|
| Engine | 1.94 MB gzipped (3.8 MB of WebAssembly) | 105 KB gzipped |
| Loader glue | 40 KB gzipped of Emscripten JavaScript | — |
| Rust bindings, linked into the app | 149 KB gzipped | — |
| Engine load | 69 ms | — |
| First page at 1400 px | 89 ms — 7 ms to parse, 44 ms to render, 29 ms to paint | — |

So: fast enough to render on the main thread, and twenty times the download. The engine and its
glue are fetched when a sheet is first opened and cached at runtime rather than precached, which
is the rule sheets already follow — a device that has never downloaded a sheet has nothing to
render. The 149 KB of bindings cannot be deferred that way, because they are linked into the
application module; if that proves too much for the shell, the renderer moves into a second
WebAssembly module loaded on demand.

Wiring is not free: Pdfium has to be handed to the bindings from JavaScript
(`initialize_pdfium_render(engine, bindings, false)`) once both modules are up, which means a
custom Trunk initialiser rather than the default loader.

The interface exists so the decision stays reversible. If the payload proves wrong in practice,
the engine moves to a worker or is replaced, and no screen changes.

## Unchanged

- **Every business rule and every acceptance criterion in every feature document.** This change
  request adds no behaviour and removes none. A screen that shows "not downloaded · 17 KB" today
  shows it after.
- **The HTTP contract**: thirty endpoints, unchanged paths, payloads, status codes and error codes.
- **The database design**: one `control.sqlite` plus one file per workspace, no `workspace_id` in
  any content table, `sync_counter` under `BEGIN IMMEDIATE`, WAL, the same migrations.
- **The sync algorithm**: client-generated UUIDv7, ordered outbox, per-field last-writer-wins,
  tombstones winning, `applied_ops` idempotency, the pin and eviction policy.
- **The local storage layout**: IndexedDB named `aurum-<workspace id>` with one store per synced
  table, sheet files in OPFS, the same `localStorage` keys. The end-to-end suite reads these, and
  keeping them keeps the suite valid across the rewrite.
- **The object-store layout**: `sheets/{workspace}/{sha256}.pdf`, `assets/{workspace}/{sha256}.{ext}`.
- **The signalling protocol**: SDP and ICE only, membership checked at the handshake, one guest per
  room, nothing written to disk.
- **Offline-first behaviour**, roles and their meanings, local-only mode and the claim.

## Impact

| Area | Impact |
|---|---|
| Affected features | All ten. None changes behaviour; each names a stack in its *Frontend* or *Backend* section, and those nouns move |
| Overview | Supersedes the *Stack* paragraph and the *Sync service* actor description in [overview.md](../overview.md) on approval |
| Existing data | **None kept.** No deployment exists; development databases are recreated from the migrations, so there is no compatibility burden on password hashes, encrypted TOTP secrets, token formats or on-device storage |
| Breaking changes | None visible to a client: same URLs, same payloads. Devices re-sign in and re-sync because the databases are recreated, not because the contract moved |
| Verification | The twelve end-to-end scripts in `e2e/` are the acceptance gate. They are black-box, so the same suite runs against the PHP server and the Rust one, and against the React client and the Leptos one |

## Sequence

Three cutovers, each leaving the app working and the end-to-end suite green.

1. **`crates/core`.** Port the rules with their tests — the 87 Vitest cases become native tests —
   and prove the port with a differential harness that runs the same inputs through the existing
   TypeScript and the new Rust and compares outputs, before either implementation is deleted.
2. **`crates/api`.** Serve the same contract, verified by running the whole end-to-end suite with
   the *unchanged React client* pointed at it. The PHP tree is deleted only once that is green.
3. **`crates/web`.** Built to parity behind the same suite, then swapped in one step: a page cannot
   be half React and half Leptos, so this cutover is atomic by nature.

## Acceptance criteria

1. `crates/core` compiles for the host and for `wasm32-unknown-unknown`, and the same functions
   serve the server and the client — no rule is implemented twice anywhere in the workspace.
2. The differential harness reports no divergence between the TypeScript and the Rust for chart
   parsing, transposition, slide building, sheet selection, ranks, search ranking and the merge
   rules, over the fixture corpus and 10,000 random cases per rule.
3. Every one of the twelve end-to-end scripts passes against the Rust API with the React client,
   and again against the Leptos client, without a single assertion being weakened.
4. A cold, offline launch of the installed app reaches the library in under two seconds, as the
   PWA feature already requires — measured with the WebAssembly shell precached.
5. Opening a sheet on a device that has never opened one fetches the PDF engine once, caches it,
   and renders the page; a device that has it cached renders with the radio off.
6. `cargo clippy -- -D warnings` and `cargo fmt --check` pass across the workspace.
7. The deployed artefact is one binary plus a directory of static assets; `docker compose up`
   still provides a working stack with an object store and a mail catcher, and no PHP.

## Progress

**`crates/core` is done.** Every rule listed above is ported, with 157 native tests, and it
compiles for the host and for `wasm32-unknown-unknown`.

Acceptance criterion 1 is demonstrated rather than asserted: the axum binary serves the synced
table list out of `aurum_core::sync::schema`, and the Leptos client — the same crate compiled to
WebAssembly — renders `[F]Amazing [G]grace` transposed into two keys in a browser, spelling the
fourth `Gb` in Db and `F#` in B. One implementation, two shells, and no table of accidentals
anywhere.

Acceptance criterion 2 is met by `differential/`, which runs 150,000 generated cases per run
through three implementations at once: the TypeScript client, the PHP server driven through the
real `SyncService` over a real SQLite workspace built from the real migrations, and the Rust.
Each rule is checked against whichever half it was ported from. No divergence. It found four
defects that had already shipped — two in the TypeScript, since fixed there, and two in the new
Rust.

Measured at the end of the phase, for the questions below:

| | Size |
|---|---|
| Client shell (WebAssembly) | 42.8 KB gzipped, 95.8 KB raw |
| Client glue (JavaScript) | 5.5 KB gzipped |
| Server binary | 1.18 MB |

That shell is the rules plus Leptos plus a router-less page, not the app; the number is a floor,
not an answer. It is recorded here so the growth is visible as the screens arrive.

## Open questions

- [ ] Does the WebAssembly shell stay small enough that the two-second offline launch holds on a
      mid-range phone, or does the client need route-level code splitting?
- [ ] Should the search index move into a WebAssembly worker as it is today, or is it fast enough
      in Rust on the main thread to delete the worker entirely?
- [ ] Is `wasm-bindgen-test` in a headless browser worth wiring into CI, or is the end-to-end suite
      sufficient cover for the browser-bound layers?
