# evault — documentation site

This directory is the Astro + Starlight source for the docs published at
**https://stescobedo.github.io/hide-env-keys/**.

It is deployed automatically by [`.github/workflows/deploy-docs.yml`](../.github/workflows/deploy-docs.yml) on every push to `master` that touches `docs/**`, on every published release, or on a manual `workflow_dispatch`.

## Local dev

Requires Node.js 20 or later.

```bash
cd docs
npm install
npm run dev       # local preview at http://localhost:4321/hide-env-keys/
npm run build     # produce ./dist/ (what the workflow uploads)
npm run preview   # serve ./dist/ locally
```

## Structure

```
docs/
├── astro.config.mjs                 # site, base, sidebar, integrations
├── package.json
├── public/
│   ├── favicon.svg
│   └── logo.svg
├── src/
│   ├── content/
│   │   └── docs/                    # one .md(x) file per page
│   │       ├── index.mdx
│   │       ├── getting-started.md
│   │       ├── tui/…
│   │       ├── cli/…
│   │       └── reference/…
│   ├── content.config.ts            # Starlight content collection bindings
│   └── styles/custom.css            # palette overrides
└── tsconfig.json
```

## Adding a page

1. Drop a `.md` or `.mdx` file under `src/content/docs/<section>/`.
2. Give it frontmatter with at least `title` and `description`.
3. Add it to the sidebar in `astro.config.mjs` so it appears in the navigation.
4. Push to `master`; the workflow rebuilds and redeploys.

## Why Astro + Starlight

Search built-in (Pagefind), dark mode, sidebar generated from config, edit-on-github links, last-updated footers, zero-config dark mode. Same stack as Bun, Cloudflare, Biome, and many other projects' docs sites — picked here for consistency with that ecosystem.

## Why this directory uses npm, not pnpm

The rest of the repo's wider plan (the `npm/` wrapper) uses plain `npm`. Keeping the docs site on the same package manager removes one cognitive switch. If you switch to pnpm later, update the workflow's setup step and the `cache: npm` field to `cache: pnpm`, and replace `npm ci` / `npm install` with the pnpm equivalents.
