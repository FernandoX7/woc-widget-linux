# Linux porting history

This project ports the behavior and visual language of the MIT-licensed
[WoC Player Count for macOS](https://github.com/FernandoX7/woc-widget) while replacing macOS menu-bar
and SwiftUI integration with Linux-native components.

The implementation progressed from the `wockit` domain model and hardened providers through local
history, analytics, alerts, Tauri polling and lifecycle, SNI tray integration, the Svelte dashboard,
packaging, and finally the native COSMIC applet. The durable outcomes are captured in the current
architecture, privacy, installation, packaging, COSMIC, and development documents.

Important preserved decisions include honest cached/unavailable provenance, no history backfill,
baseline-safe alerts, market/realm failure isolation, a static tray menu for compatibility,
visibility-gated candles, explicit external-link validation, XDG persistence, and duplicate COSMIC
panel suppression.

The original phase plans, starter prompts, internal QA transcripts, raw evidence, temporary paths,
and repetitive status tables were intentionally removed from the public tree. They were useful
during development but are neither current documentation nor safe publication artifacts. The Git
history containing those records must remain private; see the publication handoff for creating a
sanitized public history from the final tree.
