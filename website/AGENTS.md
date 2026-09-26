## Development

Package manager is **bun** (`bun install`, `bun run dev`, `bun run build`).
The dev server runs supervised under **bgrun** (job `website-dev`, foreground
via `ASTRO_DEV_BACKGROUND=0`) — do NOT start `astro dev --background`
alongside it; that creates a second lock-held server. Manage with
`bgrun status|diff|kill website-dev`. Restart the job after changing
`astro.config.mjs` (integrations only load at startup). Serves at
`http://localhost:4321/podbox/`.

If all islands render static (clicks do nothing) in dev, the Vite dep
optimizer cache went stale (classic symptom: `504 Outdated Optimize Dep`
for `lucide-react` in the browser console) — kill the job, run
`bun run dev:fresh`, and re-check with a real click, not a screenshot.

Content source of truth is `../docs/*.md`; `bun run build` (prebuild
`scripts/sync-content.ts`) syncs it into `src/content/docs/` plus assets,
`public/install.sh`, and the regenerated search index — never edit the
synced copies by hand.

## Documentation

Full documentation: https://docs.astro.build

Consult these guides before working on related tasks:

- [Adding pages, dynamic routes, or middleware](https://docs.astro.build/en/guides/routing/)
- [Working with Astro components](https://docs.astro.build/en/basics/astro-components/)
- [Using React, Vue, Svelte, or other framework components](https://docs.astro.build/en/guides/framework-components/)
- [Adding or managing content](https://docs.astro.build/en/guides/content-collections/)
- [Adding styles or using Tailwind](https://docs.astro.build/en/guides/styling/)
- [Supporting multiple languages](https://docs.astro.build/en/guides/internationalization/)
