# Installation

Beta Debian and AppImage packages are published on
[GitHub Releases](https://github.com/FernandoX7/woc-widget-linux/releases). They currently target
Ubuntu and Pop!_OS; behavior on other Linux desktops is best effort while testing continues.

## Install a beta package

Download both the package and `SHA256SUMS` from the release page, then verify the download from its
directory:

```sh
sha256sum --check SHA256SUMS --ignore-missing
```

Install the Debian package with `sudo apt install ./<downloaded-package>.deb`. For the AppImage,
run `chmod +x <downloaded-file>.AppImage` and then launch it. The Debian package is the preferred
choice on Ubuntu and Pop!_OS because the package manager handles its runtime dependencies.

## Build and run from source

Ubuntu/Pop!_OS prerequisites:

```sh
sudo apt update
sudo apt install build-essential curl libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev libssl-dev libgtk-3-dev librsvg2-dev \
  libxdo-dev pkg-config
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Install a current Node.js release supported by Vite 8 (Node 20.19+ or 22.12+), then:

```sh
git clone https://github.com/FernandoX7/woc-widget-linux.git
cd woc-widget-linux/woc-app/ui
npm ci
cd ..
ui/node_modules/.bin/tauri dev
```

This is a development launch. For a production binary or package, use the commands in
[PACKAGING.md](PACKAGING.md). A plain debug binary expects `http://localhost:1420`; launch it via
`tauri dev` so Vite is running.

## Debian package and AppImage

Build both from `woc-app/` after `npm ci`:

```sh
ui/node_modules/.bin/tauri build --bundles deb,appimage
```

Inspect the generated names beneath `target/release/bundle/`. Install a locally built Debian
package with `sudo apt install ./path/to/package.deb`, or mark the AppImage executable and run it.
Locally built artifacts are unsupported; use the checksummed GitHub Release assets when possible.

Before uninstalling or moving an AppImage, disable **Launch at login**. Package removal preserves
settings and history. Remove Debian packages with `sudo apt remove <installed-package-name>` and
delete an AppImage to remove it.

## Local Flatpak

The manifest consumes a host-built release binary and is not suitable for Flathub submission:

```sh
cd woc-app/ui
npm ci && npm run build
cd ../..
cargo build --release -p woc-app --features tauri/custom-protocol
flatpak run org.flatpak.Builder --user --install --force-clean build/flatpak \
  woc-app/flatpak/io.github.fernandox7.wocplayercount.yml
flatpak run io.github.fernandox7.wocplayercount
```

The local Flatpak build has completed previously, but the package is not published or
clean-environment verified. Uninstall with `flatpak uninstall --user
io.github.fernandox7.wocplayercount`; add `--delete-data` only when local data should also be erased.

## COSMIC applet

The native applet is a separate Debian package and depends on the main Debian package:

```sh
sh woc-cosmic-applet/package-deb.sh 0.1.0
sudo apt install ./target/debian/woc-widget-cosmic-applet_0.1.0_$(dpkg --print-architecture).deb
```

Installation was not performed on the development host because privileged package operations were
out of scope. See [COSMIC.md](COSMIC.md).

## Data preservation and complete removal

Uninstallers intentionally retain `${XDG_CONFIG_HOME:-$HOME/.config}/woc-widget` and
`${XDG_DATA_HOME:-$HOME/.local/share}/woc-widget`. The exact deletion commands and Flatpak paths are
in [PRIVACY.md](../PRIVACY.md).
