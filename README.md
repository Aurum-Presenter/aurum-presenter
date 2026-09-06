# Aurum Presenter

Offline-first, installable web app for managing chord charts and sheet music, and for
presenting lyrics on a multi-screen stage setup.

The specification lives in [`docs/SDD/`](docs/SDD/index.md) and is written before any code.
Start with the [system overview](docs/SDD/overview.md), then the feature documents and change
requests listed in the [index](docs/SDD/index.md).

## Layout

```
backend/     PHP 8.4 · Mezzio 3 · SQLite      API only, no server-rendered views
frontend/    React 19 · TypeScript · Vite     the installable PWA
compose.yaml the backend stack: API, MinIO, Mailpit — and no database service
```

## The shape of the backend

There is **one SQLite file per workspace**, plus one `control.sqlite` holding accounts,
sessions, workspaces and memberships. Two consequences run through the whole codebase:

- **Content tables carry no `workspace_id`.** The file is the scope, so a query cannot forget
  the predicate — there is no predicate. This is what replaced Postgres row-level security.
- **`WorkspaceMiddleware` opens the file only after verifying membership**, and attaches the
  connection to the request. A handler that was not granted the workspace is never handed a
  connection to it. Permissions are declared on the route
  (`#[Route(options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceWrite])]`) and
  resolved before dispatch, so a handler that runs is already authorized and must not re-check.

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
`make logs`, `make shell`, `make test`, `make web-test`, `make stan`. Run `make` on its own for
the full list.

Two processes, not one: `api` serves the HTTP API, and `signal` runs the stage-pairing relay,
which holds WebSockets open — something a request lifecycle cannot do. The relay carries SDP and
ICE between two devices on a LAN and nothing else; session state never reaches it.

## Status

Specification **approved**, and every document in the index is now implemented: accounts,
workspaces and invitations; the song library with local search and import; chord charts with
key-signature-aware transposition, capo and Nashville numbers; sheet PDFs with offline files and
annotations; sets, reader mode and a printable pack; live presentation with audience output,
stage view and LAN pairing; the sync engine with its conflict review and storage controls; and
the installable PWA.

Two properties run through all of it. **Charts are parsed and transposed entirely on the
device** (`frontend/src/chart/`): the server stores the ChordPro text and replicates it, and
never re-letters a chord — which is what lets two members read the same chart in two different
keys, offline, from one byte-identical body. And **nothing waits for the network**: every read
and every write goes to IndexedDB first, and sync is a background reconciliation that can fail
without a single screen changing.
