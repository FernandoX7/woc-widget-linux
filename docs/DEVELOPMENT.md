# Development

## Prerequisites

Ubuntu/Pop!_OS packages:

```sh
sudo apt install build-essential curl libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev libssl-dev libgtk-3-dev librsvg2-dev \
  libxdo-dev pkg-config
```

Use stable Rust with `rustfmt` and `clippy`, plus Node 20.19+ or 22.12+ (the lockfile is tested with
modern Node). Install JavaScript dependencies with `npm ci`; do not replace the lockfile with an
unreviewed `npm install` update.

## Run and verify

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd woc-app/ui
npm ci
npm run check
npm run build
npx playwright install --with-deps chromium
npx playwright test
```

For the integrated development app:

```sh
cd woc-app
ui/node_modules/.bin/tauri dev
```

The CLI belongs to `woc-app/ui/node_modules`; `cargo tauri` is not assumed. Tauri starts Vite on
port 1420. Consequently, `cargo run -p woc-app` produces a debug binary that still expects that dev
URL. A production application must be built through Tauri:

```sh
cd woc-app
ui/node_modules/.bin/tauri build --bundles deb,appimage
```

Playwright uses synthetic URL-selected states and a local Vite server. It must never use real user
history or silently enable fixture transport in a production Tauri session. The optional live
service tests are ignored by default because external providers are not deterministic.

See [ARCHITECTURE.md](ARCHITECTURE.md) for ownership boundaries and [CONTRIBUTING.md](../CONTRIBUTING.md)
for privacy and desktop verification rules.
