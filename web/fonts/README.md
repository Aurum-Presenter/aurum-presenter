# The client's typefaces

Archivo and JetBrains Mono, as variable `woff2`, latin and latin-ext subsets — 112 KB for all four.

They are **served from this origin**, copied into the distribution by Trunk and precached by the
service worker with everything else. This is not a preference: the app has to start from a
precached shell with the radio off, and a `@font-face` pointing at a font CDN is a request that
fails on a stage with no signal.

Two glyphs fall outside both subsets and render in the fallback face: `♯` (U+266F) and `♭`
(U+266D), which a pasted chart can contain — the app's own renderer writes accidentals as ASCII
`#` and `b` — and the arrows in `← Library` and `Next →`.

Both families are licensed under the SIL Open Font License 1.1:

- Archivo — Omnibus-Type, <https://github.com/Omnibus-Type/Archivo>
- JetBrains Mono — JetBrains, <https://github.com/JetBrains/JetBrainsMono>

To refresh a face, fetch the subset `woff2` that Google Fonts serves for the family and replace
the file; the `unicode-range` declarations in `web/app.css` must keep matching the subsets.
