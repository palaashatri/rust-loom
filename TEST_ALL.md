# Test All

## One command (orchestrated)

```bash
cd loom-bootstrap
bash scripts/test-all.sh        # cargo test --workspace in every cargo repo
bash scripts/fmt-all.sh         # cargo fmt --check everywhere
bash scripts/clippy-all.sh      # clippy --all-targets -- -D warnings everywhere
```

Latest results (2026-08-01): fmt PASS (5 repos), clippy PASS (0 diagnostics in loom crates),
tests PASS — loom-core 84, loom-writer 6, loom-sheets 12, loom-vision + loom-plugin-sdk green.

## Per-repo

```bash
cd loom-core && cargo test --workspace          # 84 passed / 0 failed
cd loom-core && cargo clippy --all-targets -- -D warnings   # 0 errors
cd loom-core && cargo fmt --check
# same pattern in loom-writer, loom-sheets, loom-vision, loom-plugin-sdk
```

## Visual regression

```bash
cd loom-core && cargo test -p loom-ui visual    # smoke-window baseline compare
# regenerate a baseline deliberately:
LOOM_SNAPSHOT_UPDATE=1 cargo test -p loom-ui visual
cd loom-bootstrap && bash scripts/visual-qa-all.sh   # app screenshots vs design-bible baselines
```

## Offline (no network)

```bash
cd loom-bootstrap
bash scripts/offline-test.sh --mode-a           # host, cargo --offline (needs warm registry cache)
bash scripts/docker-offline-test.sh             # Docker, --network none
```

## Known cross-platform caveat

The loom-ui visual baseline and app baselines are macOS-font-dependent. Inside the CI
container they differ slightly (glyph antialiasing); see KNOWN_LIMITATIONS.md.
