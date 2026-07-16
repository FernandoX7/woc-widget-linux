# COSMIC integration

WoC Player Count offers two COSMIC integrations:

- The native `woc-cosmic-applet`, installed separately and added through **COSMIC Settings →
  Desktop → Panel → Panel applets**.
- The generic StatusNotifierItem tray, which supplies an icon and menu when the native applet is
  not configured.

The native applet displays the configured Players, Token, combined, or Full label. It reads
`${XDG_CONFIG_HOME:-$HOME/.config}/woc-widget/settings.json`, applies `wockit` freshness rules, and
opens or focuses the Tauri dashboard when pressed. The main application queries COSMIC panel
configuration and suppresses its generic item when the native applet is configured, including
after tray recreation.

Local Pop!_OS 24.04 COSMIC testing verified applet discovery, activation, settings sharing,
continuous price freshness behavior, and duplicate generic-tray suppression. Debian installation,
upgrade, and removal of the separate applet package were not performed with elevated privileges.

COSMIC notification surfaces may omit explicit action buttons. The default/body action still opens
the dashboard; mute and disable controls remain available in Settings.
