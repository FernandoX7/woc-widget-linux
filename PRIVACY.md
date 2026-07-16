# Privacy

WoC Player Count has no account system, telemetry, analytics, advertising, crash reporter, or
companion backend. It never requests World of ClaudeCraft credentials or a wallet connection.

## Network data flow

The app makes direct HTTPS requests to World of ClaudeCraft public status/community endpoints,
DEX Screener for spot and rolling market metrics, and GeckoTerminal for OHLCV candles. Opening a
community or market link delegates that URL to the system browser. Providers receive ordinary
connection metadata, including the device's public IP address, time, user agent/TLS information,
and the requested resource. Their own privacy terms apply.

## Local storage

Native packages and AppImages store:

| Data | Location |
| --- | --- |
| Settings | `${XDG_CONFIG_HOME:-$HOME/.config}/woc-widget/settings.json` |
| Player history | `${XDG_DATA_HOME:-$HOME/.local/share}/woc-widget/history.json` |
| Recovery snapshot | `${XDG_DATA_HOME:-$HOME/.local/share}/woc-widget/history.json.backup` |
| Optional autostart entry | `${XDG_CONFIG_HOME:-$HOME/.config}/autostart/io.github.fernandox7.wocplayercount.desktop` |

Flatpak remaps these paths below `~/.var/app/io.github.fernandox7.wocplayercount/`. Settings include
display, polling, alert, quiet-hour, cooldown, and local-record choices. History contains timestamp
and aggregate player-count observations, not player identities. It is normalized to a rolling
seven-day window. The backup sidecar preserves the prior valid snapshot for recovery.

## Export and deletion

Export writes CSV or JSON history only to a path selected by the user. **Clear History** removes
both history snapshots and pending in-memory history. Package removal preserves user data by
design. To erase everything after quitting:

```sh
rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/woc-widget"
rm -rf "${XDG_DATA_HOME:-$HOME/.local/share}/woc-widget"
rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/io.github.fernandox7.wocplayercount.desktop"
```

For Flatpak, remove app data with `flatpak uninstall --delete-data
io.github.fernandox7.wocplayercount`. No server-side companion profile exists to delete.
