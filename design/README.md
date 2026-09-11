# Design canvas

The source of the stage-dark redesign: one `.dc.html` per screen, laid out by `canvas.json`.

| File | Screen | Frame |
|---|---|---|
| `Main.dc.html` | Library | 1440×900 |
| `Console.dc.html` | Presentation console | 1440×900 |
| `Reader.dc.html` | Chart reader, tablet portrait | 900×1200 |
| `Audience.dc.html` | Audience output | 1280×720 |

These are **mockups, not the app**: static markup with inline styles, drawn to argue a direction.
Nothing here is compiled, imported by `crates/web`, or served. The direction they propose —
near-black ground, gold as brand and selection, red for live and gold for next, Archivo with
JetBrains Mono for keys and tempo — is what would move into Tailwind theme tokens if it is
accepted.

The published canvas is assembled from these files by the `/design` skill's seeder, which bakes an
editor around them. That output is a 2.5 MB single file, regenerated on every change, so it is
gitignored: these five files are the thing worth keeping.
