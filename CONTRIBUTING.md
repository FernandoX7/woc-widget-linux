# Contributing

Thank you for improving WoC Player Count. Keep changes focused, preserve honest provenance, and
run the relevant verification before opening a pull request.

## Setup and verification

Install the prerequisites in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md), then run:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd woc-app/ui
npm ci
npm run check
npm run build
npx playwright install chromium
npx playwright test
```

Use `ui/node_modules/.bin/tauri dev` from `woc-app/` for the integrated application.

## Boundaries and expectations

- Put portable models, providers, history, analytics, alert policy, and formatting in `wockit`.
- Keep Linux lifecycle, persistence, notifications, and tray behavior in `woc-app`.
- Keep UI fixtures deterministic and synthetic. Never fabricate production history or silently
  replace missing observations with fixture data.
- Preserve the live/cached/loading/unavailable distinction and outage eligibility rules.
- Add tests for behavior changes and avoid unnecessary dependencies.
- For panel, notification, autostart, or window changes, state the exact desktop/session tested.
  Mark other environments expected or unverified rather than implying a broad matrix result.
- Screenshots must be cropped to the app, stripped of metadata, and reviewed for terminals,
  messages, usernames, hostnames, paths, notifications, and unrelated desktop content.
- Never commit credentials, local settings/history, build output, caches, raw QA evidence, or agent
  prompts. Use synthetic redacted evidence when a durable artifact is genuinely needed.

Use Conventional Commits, for example `fix(tray): preserve cached label provenance` or
`docs: clarify Flatpak limitations`. Pull requests should explain user impact, tests run, desktop
verification, packaging impact, and privacy/security impact.
