# Testing

## Commands

```sh
cargo fmt --check                                  # formatting
cargo clippy --all-targets -- -D warnings          # zero warnings required
cargo test --workspace                             # all unit + integration tests
cargo build --release                              # release gate
```

## Test inventory

| Crate | Scope | What it proves |
| --- | --- | --- |
| manifest (unit) | 24 tests | validation error matrix (each rule -> its error), version matrix, serde round trip, id/mode/path helpers |
| host (unit) | 4 tests | safe-name rejection, path normalization, component-aware prefix matching |
| host (integration) | 19 tests | install+sha, double-install, traversal/absolute/symlink rejection with "nothing extracted" assertions, bad manifest, missing manifest/wasm, api mismatch, archive-bomb limit, uninstall, corrupt-dir skip, permission matrix, index regeneration |
| cli (unit) | 2 tests | fixture sources valid, generated zip installs cleanly |
| cli (integration) | 8 tests | real binary: validate ok/bad/missing, install/list/remove round trip, malicious zip rejection, usage errors |

Total: 57 tests.

## Conventions

- Security rejections assert the store is untouched afterwards.
- No test may require network, a GPU, or a committed binary fixture.
- The symlink-entry fixture is crafted as raw zip bytes (zip 0.6.6's writer
  masks file-type bits); see `make_symlink_zip` in host integration tests.
- Temp dirs are hand-rolled with Drop cleanup; never leave files behind.
