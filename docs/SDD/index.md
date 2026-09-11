# Software Design Document — Aurum Presenter

[System overview](overview.md) · Last updated: 2026-09-11

Specification lives here, one document per unit of work. Nothing is built before its document
is `Approved`.

## Features

| Feature | Added | Status | Document |
|---|---|---|---|
| Accounts, workspaces and access control | 2026-09-06 | Approved | [→](feature/2026-09-06-workspaces-and-access.md) |
| Song library — folders, songs and search | 2026-09-06 | Approved | [→](feature/2026-09-06-song-library.md) |
| Chord charts, arrangements and transposition | 2026-09-06 | Approved | [→](feature/2026-09-06-chord-charts-and-transposition.md) |
| Sheet attachments — PDFs per key | 2026-09-06 | Approved | [→](feature/2026-09-06-sheet-attachments.md) |
| Sets and setlists | 2026-09-06 | Approved | [→](feature/2026-09-06-sets.md) |
| Offline storage and sync engine | 2026-09-06 | Approved | [→](feature/2026-09-06-offline-storage-and-sync.md) |
| PWA installation and offline app shell | 2026-09-06 | Approved | [→](feature/2026-09-06-pwa-installation.md) |
| Presentation — live sessions (parent) | 2026-09-06 | Approved | [→](feature/2026-09-06-presentation.md) |
| Presenter output — audience screens | 2026-09-06 | Approved | [→](feature/2026-09-06-presenter-output.md) |
| Stage view — local window and paired device | 2026-09-06 | Approved | [→](feature/2026-09-06-stage-view.md) |

## Change requests

| Change | Added | Status | Affects | Document |
|---|---|---|---|---|
| Self-hosted PHP backend on SQLite, replacing Supabase | 2026-09-06 | Approved | Workspaces, Sync engine, Song library, Chord charts, Sets, Sheets, PWA | [→](change-request/2026-09-06-sqlite-backend.md) |
| Self-hosted email + password sign-in with TOTP | 2026-09-06 | Approved | Workspaces and access | [→](change-request/2026-09-06-self-hosted-auth-totp.md) |
| Sheet PDFs on S3-compatible object storage | 2026-09-06 | Approved | Sheet attachments, Sync engine | [→](change-request/2026-09-06-s3-sheet-storage.md) |
| Self-hosted WebSocket signalling for stage pairing | 2026-09-06 | Approved | Stage view | [→](change-request/2026-09-06-lan-websocket-signalling.md) |
| One language for both halves — Rust and WebAssembly | 2026-09-08 | Approved | Every feature (stack only; no behaviour changes) | [→](change-request/2026-09-08-rust-rewrite.md) |
| Stage dark — one interface vocabulary for the client | 2026-09-11 | Approved | Library, Chord charts, Presentation, Presenter output, Stage view, PWA (appearance only) | [→](change-request/2026-09-11-stage-dark-interface.md) |
