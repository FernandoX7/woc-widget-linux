# Repository guide

WoC Player Count is a Linux panel/tray companion for World of ClaudeCraft. Public architecture,
development, privacy, packaging, and COSMIC behavior are documented under `docs/`.

Preserve these invariants: cached data is never labeled live; market failures never change realm
availability; history records only real observations and retains seven days; alerts require a
baseline; the tray menu stays static; dashboard-only polling stops when hidden; external links are
allowlisted; and the app has no telemetry.

Use Conventional Commits. Stage explicit paths rather than `git add -A`. Add tests for behavior
changes and identify exactly which Linux desktop/session was verified for integration changes.
Never commit local settings/history, raw desktop screenshots, terminal or chat evidence, build
outputs, caches, secrets, private paths, or internal agent instructions.

Run the complete verification documented in `docs/DEVELOPMENT.md` before publication.
