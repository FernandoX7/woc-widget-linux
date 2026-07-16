# Packaging status

No package is currently distributed through GitHub Releases, Flathub, a Debian repository, or an
automatic updater.

| Format | Repository support | Verification | Publication status |
| --- | --- | --- | --- |
| Debian | Tauri bundle config, desktop file, AppStream metadata, hicolor icons | Built and structurally inspected locally; privileged install/upgrade/remove deferred | Not published |
| AppImage | Tauri bundle config and desktop assets | Built and inspected locally; clean-host runtime deferred | Not published |
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

Before a first release, verify each artifact on a clean supported distribution, inspect package
contents and dependencies, exercise install/upgrade/uninstall and autostart, and publish checksums.
Do not call the existing local artifacts official releases.
