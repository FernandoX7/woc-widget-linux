## Summary

Describe the user-visible result and the architectural component changed.

## Verification

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `npm ci && npm run check && npm run build`
- [ ] `npx playwright test`
- [ ] Desktop/session behavior was tested or clearly marked unverified

List exact commands, distribution, desktop, display protocol, and installation method.

## Safety

- [ ] Live/cached/unavailable provenance remains honest
- [ ] Fixtures and history are synthetic; no production data was fabricated
- [ ] No secrets, personal paths, chats, raw desktop evidence, or generated output is included
- [ ] Network, privacy, permissions, packaging, and dependency changes are explained
