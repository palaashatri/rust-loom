# Fuzzing loom-package

`loom-package/fuzz/` is a self-contained `cargo-fuzz` crate that is *not* a
member of the loom-core workspace, so normal `cargo test` / CI never need the
fuzz toolchain. Run fuzzing explicitly with:

```sh
cargo +nightly install cargo-fuzz
cargo +nightly fuzz run package_read
# Correlate failures:
cargo +nightly fuzz build package_read
```

`package_read` feeds arbitrary bytes into `PackageArchive::from_bytes_with_limits`
and requires the reader to never panic and to fail in a bounded way on
truncated, malformed, and checksum-corrupted archives.

Each crash reproduces with `cargo +nightly fuzz run package_read <crash-file>`.
Failing inputs should be added to `loom-core/crates/loom-package/src/zip.rs`
as regression cases in `hostile_inputs_never_panic_and_fail_bounded`.