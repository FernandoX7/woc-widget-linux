# Packaging status

Beta Debian and AppImage packages are distributed through GitHub Releases. There is no Flathub
listing, Debian repository, or automatic updater.

| Format | Repository support | Verification | Publication status |
| --- | --- | --- | --- |
| Debian | Tauri bundle config, desktop file, AppStream metadata, hicolor icons | Automated build and structural checks; broader clean-host lifecycle testing ongoing | Beta on GitHub Releases |
| AppImage | Tauri bundle config and desktop assets | Automated build and structural checks; broader clean-host runtime testing ongoing | Beta on GitHub Releases |
| Flatpak | Local builder manifest, SNI and notification D-Bus permissions | Local build completed previously; manifest consumes a host binary | Not Flathub-ready or published |
| COSMIC applet `.deb` | Separate packaging script and control file | Package contents/lifecycle tested without privileged install; installation deferred | Not published |

Build Debian and AppImage artifacts from `woc-app/`:

```sh
ui/node_modules/.bin/tauri build --bundles deb,appimage
```

The Flatpak manifest intentionally grants network access, Wayland/fallback-X11 graphics access,
and D-Bus access to `org.kde.StatusNotifierWatcher` and `org.freedesktop.Notifications`. There is no
SNI portal. A future Flathub manifest must declare reproducible Cargo/npm sources instead of
copying `target/release/woc-widget`.

The tag-driven release workflow builds both public artifacts and publishes SHA-256 checksums.
Before promoting a beta to stable, verify each artifact on clean supported distributions and
exercise install, upgrade, uninstall, autostart, tray activation, and notifications.
