# Visual QA

Loom Vision has no GUI. Its "visual" outputs are the deterministic,
machine-checkable results of the CLI, which double as visual-regression
evidence for the reference providers:

1. **Fixture generation** (committed, reproducible):

   ```sh
   cargo run --example gen_fixture -- "Hello, Loom!" crates/loom-vision-cli/fixtures/hello.png
   ```

2. **Decode check** — the QR provider must reproduce the exact payload:

   ```sh
   cargo run --bin loom-vision -- qr crates/loom-vision-cli/fixtures/hello.png
   # expected stdout: Hello, Loom!  (exit 0)
   ```

3. **Stats sanity** — a black-on-white QR is mostly white with dark
   modules: mean luma above 100, high std, Michelson contrast 1.00.

   ```sh
   cargo run --bin loom-vision -- stats crates/loom-vision-cli/fixtures/hello.png
   ```

4. **Error visuals** — corrupted inputs must produce non-zero exits with
   stderr messages: `qr /nonexistent.png` → exit 1; tampered pack →
   `inspect-pack` prints the failing check.

Acceptance: steps 1–3 produce the documented outputs on a clean checkout,
and the fixture PNG round-trips through `qr` byte-for-byte in payload text.
Any change to the QR or stats providers must re-run this checklist and keep
the fixture and expected outputs in sync.
