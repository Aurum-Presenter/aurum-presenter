---
id: stage-dark-interface
title: Stage dark — one interface vocabulary for the client
type: change-request
status: Approved
created: 2026-09-11
updated: 2026-09-11
changes:
  - feature/2026-09-06-song-library.md
  - feature/2026-09-06-chord-charts-and-transposition.md
  - feature/2026-09-06-presentation.md
  - feature/2026-09-06-presenter-output.md
  - feature/2026-09-06-stage-view.md
  - feature/2026-09-06-pwa-installation.md
related:
  - change-request/2026-09-08-rust-rewrite.md
---

# Stage dark — one interface vocabulary for the client

## Context

The client has never had a design system. Every screen was written in stock Tailwind defaults —
the slate palette, `system-ui`, 4px corners — and navigation was carried by underlined text.
That was the right cost to pay while the rules were being built and then rewritten twice; it is
the wrong thing to leave in front of a musician.

It shows up as three concrete failures rather than as an aesthetic complaint:

- **The reading surface fights its room.** A chart is read from a music stand, often in a dim
  room, at arm's length. A white page at full brightness beside a lit stage is the wrong object,
  and the app offered no considered alternative — only whatever `prefers-color-scheme` happened
  to say.
- **Nothing says what is live.** On the presentation console, the slide the room is seeing and
  the slide that is one press away were drawn identically, distinguished by position and a small
  outline in the same colour as every other selected thing.
- **The controls are menus.** Layout, mode, key and part were four `<select>` elements. A player
  mid-set should not have to open a menu to see what their options are, let alone choose one.

## Requested change

A dark-first interface vocabulary, defined once as tokens and applied across every screen.

### The system

| | |
|---|---|
| Ground | `#0B0C0F`, with `#08090C` under the console |
| Surfaces | `#14161B` raised to `#1C1F26` |
| Hairlines | `#23262E`, and `#2B303A` for a control's own border |
| Ink | four steps: `#ECEEF2` · `#C8CDD6` · `#9BA3B0` · `#828B9A` |
| Brand and selection | gold `#E9B949` — the app is called Aurum |
| Live | red `#D43B44`, carrying white text |
| Synced / not responding | green `#3DD68C` / amber `#E0A335` |
| Type | Archivo for chrome and lyrics, JetBrains Mono for values read as values |
| Radii | 6px; controls 40px, and 44px where a finger uses them |

The ink ramp stops where it does deliberately: `#828B9A` is the last step that still clears 4.5:1
on the lightest surface it lands on. A label nobody can read is not a label, and the palette
should not contain a shade that invites one.

**Red is live, gold is next.** That pairing runs through the whole presentation surface — the two
previews, the slide cards, the running-order chips — so an operator never reads a label to learn
which is which. It is the one piece of colour in the app carrying meaning rather than emphasis,
which is why nothing else may use red.

### What changes on the screens

- **The chart reader**: key, capo and the shapes they produce are drawn as one object, because
  the app already computes them as one decision. Layout and mode become segmented controls.
- **The library**: a real table — key and tempo in the mono face, scannable down a column — with
  the duplicate-title and archived states shown rather than hidden.
- **The console**: a LIVE badge, the audience preview outlined in red, the next preview in gold,
  and the running order marking both.
- **Everything else** inherits the vocabulary through the tokens.
- **Actions stop being underlined.** An action is quiet text that brightens on hover, and the
  underline arrives with the pointer. Underlines inside prose are unchanged.

### Two theme sets, not one inverted

Dark is the designed theme. A light theme exists and is measured, not derived by inversion — an
inverted dark palette is how a light theme ends up grey and muddy — but it is secondary, and it
has not been reviewed screen by screen.

Three surfaces are exempt from theming altogether: the audience output, the stage view, and the
console's preview of them. They are screens in a room rather than app chrome, and they are black
whatever the operating system prefers.

## Unchanged

Every business rule, every acceptance criterion, and every string the app shows. This change
moves colour, type and control shape; it moves no behaviour. In particular the end-to-end suite
passes unweakened, which is the check that the vocabulary changed and the app did not: the specs
select by accessible name, by visible text, and in one place by the `font-mono` class, so a
redesign that broke any of those would fail them.

## Impact

| Area | Effect |
|---|---|
| `web/app.css` | Becomes the system: `@theme` tokens, both theme sets, the self-hosted faces |
| `crates/web/src` | Every screen names roles (`bg-surface`, `text-ink-3`) rather than shades; no `dark:` variant survives, because a token carries both themes |
| Offline shell | Archivo and JetBrains Mono are served from this origin and precached — 112 KB for four variable subsets. A `@font-face` pointing at a font CDN is a request that fails on a stage with no signal |
| Accessibility | Every text colour is measured against the surfaces it actually lands on, rather than chosen by eye |

## Acceptance criteria

1. No screen names a palette shade: every colour resolves through a token, and `dark:` appears
   nowhere in `crates/web/src`.
2. Body text clears 4.5:1 against the surface it sits on, in both theme sets.
3. The app renders its own type with the network off, from the precached shell.
4. On the console, the live slide and the next slide are distinguishable without reading a word —
   in the previews, the slide list and the running order alike.
5. The end-to-end suite passes with no assertion weakened.

## Open questions

- [ ] Should the light theme be designed screen by screen, or narrowed to a print-and-daylight
      mode for the library and the editor only?
- [ ] The stage view and audience output are black in every theme. Should the operator be able to
      choose a light audience theme for a daylit room, given the theme system already carries a
      background colour per session?
