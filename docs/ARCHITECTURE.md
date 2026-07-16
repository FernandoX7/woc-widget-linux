# Architecture

WoC Player Count separates portable policy from Linux integration and presentation.

## Components

`wockit` owns hardened HTTP behavior, provider decoding, feed provenance, history normalization and
persistence, analytics, formatting, and alert policy. It has no UI dependency. Network calls use
timeouts and response-size limits; market links and pair identity are validated before display.

`woc-app` is the Tauri v2 application. It schedules polling, keeps attempt and success timestamps,
persists settings/history, creates the dashboard, exports files, publishes the generic SNI tray,
sends desktop notifications, and reconciles XDG autostart.

`woc-app/ui` is a Svelte 5/Vite/TypeScript single-page dashboard. It renders state received through
Tauri commands/events. Test-only URL scenarios use deterministic synthetic fixtures.

`woc-cosmic-applet` is a native libcosmic process. It reads the shared settings file, applies the
same label freshness policy from `wockit`, polls the public quote/status services, and activates the
main application. The main app suppresses its generic tray when the configured COSMIC applet is
present to avoid duplicate panel items.

## Data flow

1. Pollers request realm, market, candle, and community resources independently.
2. `wockit` validates and decodes each response into typed models.
3. The shell records attempt/success provenance and stores eligible player observations.
4. The alert engine evaluates transitions only after a baseline exists. Market failures never mark
   the realm down; remote outage-eligible failures require consecutive observations.
5. A dashboard snapshot is sent to the UI. Cached values retain their cached label.

History uses a frozen JSON array of `{ "date": <ISO-8601>, "count": <integer> }`, rolling
seven-day retention, coalesced writes, and a `.backup` sidecar. Gaps remain gaps; the app never
backfills time it did not observe.

## Linux integration invariants

- The tray menu remains static: Open Dashboard, Refresh, and Quit.
- Price label modes fall back to players when price freshness expires.
- Dashboard candle polling and animation stop while the window is closed.
- Notifications include a default action opening the dashboard; action presentation is controlled
  by the desktop.
- Wayland windows are not positioned relative to a panel.
- External URLs pass explicit origin/path validation before opening.

See [PRIVACY.md](../PRIVACY.md) for storage/network details and [COSMIC.md](COSMIC.md) for the
native applet lifecycle.
