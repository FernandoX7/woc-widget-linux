# WoC dashboard UI

The Svelte dashboard includes deterministic, offline fixtures for visual QA. Fixture mode does not import Tauri or make network requests. Every run uses the fixed reference clock `2026-07-11T18:00:00.000Z`.

## Fixture scenarios

Set `VITE_WOC_PREVIEW_STATE` to one of:

- `live`
- `welcome`
- `loading`
- `cached-offline`
- `quote-only`
- `chart-only`
- `empty-history`
- `gap-history`
- `rhythm-29`
- `rhythm-30`

Set `VITE_WOC_PREVIEW_PAGE` to `overview`, `market`, or `community`. For example:

```sh
VITE_WOC_PREVIEW_STATE=cached-offline VITE_WOC_PREVIEW_PAGE=market npm run dev
```

Query parameters override the environment for quick screenshot switching:

```text
http://localhost:5173/?state=quote-only&page=market
```

The long query names `woc-preview-state` and `woc-preview-page` are also accepted. Unknown values safely select `live` and `overview`.

Fixture transport is installed automatically before the dashboard mounts whenever either preview environment variable or query parameter is present. Consumers can also import `resolveFixtureSelection`, `createFixtureSnapshot`, or `createFixtureTransport` from `src/lib/fixtures`. The transport implements the same `invoke`/`listen` contract as Tauri for `dashboard_snapshot`, `refresh_page`, and `quit_app`, plus the internal `fixture_snapshot` command used by later page phases. Overview refresh emits both `woc://status-changed` and `woc://market-changed`; the other pages emit their matching market or community event.

Overview analytics fixtures are backend-shaped outputs generated exclusively through
`wockit`. Regenerate and verify them with:

```sh
cargo run -q -p woc-app --example generate_overview_fixtures > woc-app/ui/src/lib/fixtures/overview-analytics.json
npm run qa:overview --prefix woc-app/ui
```
