# Loom Vision

Local-first computer-vision framework for the [Loom](https://github.com/loom) creative suite.

Loom Vision is a *provider framework*: applications ask for a capability
(QR detection, OCR, segmentation, ...) and receive results from whatever
provider is registered for it. Everything runs locally — no network access,
no telemetry, no remote inference, no mandatory model downloads.

## Status

`FUNCTIONAL_WITH_LIMITATIONS` (0.1.0). Two deterministic CPU reference
providers ship today: QR decoding (`rqrr`) and image statistics. Model-pack
handling (manifest parsing, checksum/path/size validation, safe install) is
complete. ONNX/Candle/GPU backends are `NOT_STARTED` (see
[ROADMAP.md](ROADMAP.md) and [TASKS.md](TASKS.md)).

## Repository layout

```
loom-vision/
  crates/loom-vision-core/   # the framework: providers, registry, model packs
  crates/loom-vision-cli/    # the `loom-vision` binary
  docs/adrs/                 # architecture decision records
```

## Quickstart

```sh
cargo build --workspace            # build everything
cargo test --workspace             # run all tests
cargo clippy --all-targets -- -D warnings   # lint gate

# Regenerate the QR fixture and exercise the CLI
cargo run --example gen_fixture -- "Hello, Loom!" crates/loom-vision-cli/fixtures/hello.png
cargo run --bin loom-vision -- qr crates/loom-vision-cli/fixtures/hello.png
cargo run --bin loom-vision -- stats crates/loom-vision-cli/fixtures/hello.png
cargo run --bin loom-vision -- bench crates/loom-vision-cli/fixtures/hello.png
```

## Using the library

```rust
use loom_vision_core::reference::QrCodeProvider;
use loom_vision_core::provider::{CapabilityProvider, ProviderInput, RunContext};

let provider = QrCodeProvider::new();
let input = ProviderInput::Image {
    width: 232, height: 232, channels: 1,
    data: gray_pixels, format: "gray".to_string(),
};
let mut ctx = RunContext::new();
let output = provider.run(&input, &mut ctx)?;   // ProviderOutput::QrDecoded { text }
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for the provider model and
[IMPLEMENTATION_GUIDE.md](IMPLEMENTATION_GUIDE.md) to add a new provider.

## Documentation index

- [ARCHITECTURE.md](ARCHITECTURE.md) — provider model, registry, model-pack lifecycle
- [BUILDING.md](BUILDING.md) — build instructions and MSRV
- [TESTING.md](TESTING.md) — test layout and gates
- [SECURITY.md](SECURITY.md) — checksums, traversal guards, archive limits
- [PERFORMANCE.md](PERFORMANCE.md) — benchmarking with `loom-vision bench`
- [ROADMAP.md](ROADMAP.md) — honest status of every planned capability
- [TASKS.md](TASKS.md) — task ledger
- [LICENSE_POLICY.md](LICENSE_POLICY.md) / [DEPENDENCIES.md](DEPENDENCIES.md)
- [docs/adrs/](docs/adrs/) — architecture decision records

## License

MIT OR Apache-2.0. See [LICENSE_POLICY.md](LICENSE_POLICY.md).
