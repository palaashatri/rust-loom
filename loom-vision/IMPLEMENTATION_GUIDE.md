# Implementation Guide — adding a new provider

## Steps

1. **Pick or add the capability id.**
   If the capability exists (see `CapabilityId`), use it. Otherwise add a
   variant to `CapabilityId` in
   `crates/loom-vision-core/src/provider.rs`, extend `as_str()`, and update
   the unit test `capability_id_string_forms_are_stable`.

2. **Implement the provider** in `crates/loom-vision-core/src/reference.rs`
   (or a new module if it needs model files). Pattern:

   ```rust
   pub struct MyProvider {
       descriptor: ProviderDescriptor,
   }

   impl MyProvider {
       pub fn new() -> Self {
           let mut d = ProviderDescriptor::new(CapabilityId::Ocr);
           d.name = "my-ocr".to_string();
           d.description = "...".to_string();
           d.input_types = vec![InputType::Image];
           d.output_schema = r#"{"type": "object"}"#.to_string();
           d.required_memory_bytes = ...;
           d.estimated_latency = Duration::from_millis(...);
           d.license = "...".to_string();          // SPDX of your implementation
           d.model_provenance = "none or description".to_string();
           // keep cancellation_support/progress_support honest
           Self { descriptor: d }
       }
   }
   impl Default for MyProvider { fn default() -> Self { Self::new() } }

   impl CapabilityProvider for MyProvider {
       fn descriptor(&self) -> &ProviderDescriptor { &self.descriptor }

       fn run(&self, input: &ProviderInput, ctx: &mut RunContext)
           -> Result<ProviderOutput, VisionError>
       {
           ctx.check_cancelled()?;
           let (w, h, channels, data, _fmt) = match input {
               ProviderInput::Image { width, height, channels, data, format } => {
                   (*width, *height, *channels, data.as_slice(), format.as_str())
               }
               _ => return Err(VisionError::UnsupportedInput),
           };
           let luma = image_to_luma_checked(w, h, channels, data, ctx)?; // checks every 8 rows
           // ... real algorithm, polling ctx.check_cancelled() in loops ...
           ctx.set_progress(1.0);
           Ok(ProviderOutput::Generic { message: String::new() })
       }
   }
   ```

3. **Expose it** from `crates/loom-vision-core/src/lib.rs` (module + re-export).

4. **Test it.**
   - Unit tests in the provider module: happy path with real input,
     unsupported-input → `UnsupportedInput`, cancel-before-run → `Cancelled`,
     descriptor sanity.
   - Cross-module flows in `tests/integration.rs` (register through
     `CapabilityRegistry`, run via `run_first_success`).

5. **CLI surface (optional).** Add a subcommand in
   `crates/loom-vision-cli/src/main.rs`; all output to stdout, errors to
   stderr, exit codes 0/1/2.

6. **Gates:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test --workspace`, `cargo build --release`.

## Provider contract (non-negotiable)

- `run` never performs I/O or network access; it returns
  `VisionError::ProviderUnavailable` if a required backend is missing.
- If the descriptor claims `cancellation_support`, poll
  `check_cancelled()` inside long loops.
- If the descriptor claims `deterministic`, identical inputs must give
  identical outputs.
- Descriptor fields must be truthful (license, provenance, memory,
  latency, backend list).
