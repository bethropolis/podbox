# Website agent notes

Layout: Astro + React + Tailwind, managed by **bun**. The workspace root
`AGENTS.md` covers the CLI side; this file is the site's own rules.

## Development

`bun install`, `bun run dev`, `bun run build`. The dev server runs
supervised under **bgrun** (job `website-dev`, foreground via
`ASTRO_DEV_BACKGROUND=0`) — do NOT start `astro dev --background`
alongside it; that creates a second lock-held server. Manage with
`bgrun status|diff|kill website-dev`. Restart the job after changing
`astro.config.mjs` (integrations only load at startup). Serves at
`http://localhost:4321/podbox/`.

If all islands render static (clicks do nothing) in dev, the Vite dep
optimizer cache went stale (classic symptom: `504 Outdated Optimize Dep`
for `lucide-react` in the browser console) — kill the job, run
`bun run dev:fresh`, and re-check with a real click, not a screenshot.
Adding dependencies (`bun add ...`) invalidates the optimizer cache the
same way, so always follow it with `dev:fresh`.

## Before calling it done

- `bun run typecheck` — types.
- `bun run check:studio` — engine parity: every Studio preset and schema
  patch must emit TOML/Quadlet the Rust engine accepts, and every surface
  (field table, defaults, enum options) must match what the engine serves.
- `bun run build` — CI runs all three (see `.github/workflows/ci.yml`).

## The engine is the only emitter

Config → TOML and config → Quadlet go through the Rust engine compiled to
wasm (`src/design/islands/studio/engine.ts`, bindings committed under
`src/wasm/`). There is no TypeScript replica to drift. If a playground or
Studio pane shows an error banner, that IS the engine's validator talking —
fix the input (or the engine), never paper over the check with string
matching.

## New config fields (a schema change upstream touches here too)

One change, all steps:

1. `crates/podbox/src/config/` type + validation + codegen + docs.
2. `website/src/design/islands/studio/schema.ts` field table + defaults.
3. `useStudioState.ts` / `reducer.ts` state key.
4. the relevant `panels/` control.
5. a `check-studio-parity.ts` case.
6. wasm bindings rebuilt with `../scripts/build-wasm.sh` and committed
   in the same change (CI has no Rust toolchain).

## Generated files — never edit by hand

- `src/content/docs/`, doc assets, `src/design/searchIndex.ts` —
  written by `scripts/sync-content.ts` (prebuild hook). Content source of
  truth is `../docs/*.md`.
- `src/wasm/` — `../scripts/build-wasm.sh`.
- `public/install.sh`, `public/og-image.png` — gitignored; regenerated on
  every build. The og:image render needs `rsvg-convert`; CI installs
  `librsvg2-bin` precisely because a skipped PNG 404s silently.
- `public/assets/*.svg` — `../scripts/gen-diagrams.py`.

## Studio presets

`src/design/islands/studio/presets/*.ts` are full state overrides. Each
must emit exactly what a clean file looks like: a preset that sets a
deprecated key (`lifecycle.on_stop`) or one that contradicts validation
puts the warning/error into every exported config downstream. Before
adding an override, run `bun run check:studio` and eyeball the emitted
TOML for boilerplate a default would have skipped.

## Sharing state

Share links, imports and exports round-trip through `share.ts` /
`parseImport.ts` against the defaults in `schema.ts` — adding a state key
means deciding what happens when an old share link is missing it
(defaults apply), and new keys must survive the `reducer-keys` parity
check.

## Astro reference

Full documentation: https://docs.astro.build

Consult these guides before working on related tasks:

- [Adding pages, dynamic routes, or middleware](https://docs.astro.build/en/guides/routing/)
- [Working with Astro components](https://docs.astro.build/en/basics/astro-components/)
- [Using React, Vue, Svelte, or other framework components](https://docs.astro.build/en/guides/framework-components/)
- [Adding or managing content](https://docs.astro.build/en/guides/content-collections/)
- [Adding styles or using Tailwind](https://docs.astro.build/en/guides/styling/)
- [Supporting multiple languages](https://docs.astro.build/en/guides/internationalization/)
