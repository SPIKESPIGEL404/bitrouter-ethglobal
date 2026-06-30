# Contributing to the docs

The published documentation site ([bitrouter.ai/docs](https://bitrouter.ai/docs))
is rendered from this `docs/` folder. The site repo (`bitrouter-docs`) syncs this
tree at build time — so docs live next to the code they describe and ship in the
same PR.

## What publishes

The published tree is exactly what [`docs/meta.json`](./meta.json) lists. Anything
not referenced there (e.g. `superpowers/`, `awesome-submissions/`) stays internal
and is never synced to the site. The API reference under `reference/*` is
**generated** from the BitRouter Cloud OpenAPI spec — do not hand-author operation
pages; only `reference/index.md` is authored here.

## Authoring contract (plain Markdown)

Pages are plain Markdown (`.md`), not MDX with imports. The sync enforces this:

1. **Frontmatter** — every page needs `title:` (and ideally `description:`). A
   `sourceHash:` is managed automatically; don't hand-edit it.
2. **No `import` / `export` lines.** A whitelisted set of components is available
   globally without imports: `Callout`, `Tabs`/`Tab`, `Cards`/`Card`, and (on the
   relevant pages) `ModelsTable`, `CalInline`. Any other `<Capitalized>` tag fails
   the sync.
3. **Callouts** — prefer GitHub-style `> [!NOTE]` / `> [!WARNING]` blockquotes, or
   the `<Callout>` component.
4. **Internal links** are site paths without extensions: `/docs/features/byok`,
   not `./byok.md`.
5. **Translations** — a Chinese page is `<name>.zh.md` beside the English
   `<name>.md`. If you change an English page without updating its `.zh.md`, the
   sync flags the translation as stale (it won't block, but it's visible).

## Adding a page

1. Create `docs/<section>/<name>.md` (and `<name>.zh.md` if translating).
2. Add `<name>` to that section's `meta.json` `pages` list in the position you
   want it to appear in the nav.
3. Open a PR. The docs site picks it up automatically once merged to `main`.
