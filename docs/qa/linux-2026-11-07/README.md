# PR #21 review resolution and status ledger update — 2026-11-07

Scope: Loom Sheets, Writer and Present, plus the ledger and documents for every application. Agent-run evidence on Linux (source, unit and integration tests, and the CI gates). This pass ran no native GUI session and produced no new screenshots. No application's product status changed: Sheets, Writer and Present remain `ACCEPTANCE_BLOCKED`, and Photo, Motion, Video, Studio and Encode remain `LOCKED`.

## Commits in this pass

| Commit | Change |
|---|---|
| `cce0fbb9` | Sheets: embedded images are refused from their header before decoding when the declared size exceeds 64 MiB decoded; the decoder's allocation limit is 128 MiB; regressions added. Addresses the PR #21 oversized-image thread. |
| *(this docs commit)* | Ledger and documents: `docs/workflow/current.md` (live table and per-application done/pending), `writer.md`, `repairs.md`, `governance-debt.md`, `docs/audits/uiux-2026-10-27.md` (new PR #21 review resolution section), this record. |

Earlier 2026-11-06 commits on the same branch, referenced by the ledger: `9f40cf08` (table-cell wrapping, Markdown paste, hover), `1bfb0b84` (caret after a table-ending paste, font-field and icon-button hover), `6d9499bf` (evidence record).

## PR #21 review threads

| Thread | Outcome | Evidence |
|---|---|---|
| Writer P1 — footnote continuation order | Resolved (reviewer's recheck agreed) | `pdf_note_continuation_pages_stay_adjacent_to_their_reference_page` |
| Present P2 — insertion points | Resolved. The reviewer could not find the 12-insertion test on 2026-11-02; it is on the PR head. | `twelve_mixed_insertions_avoid_occupied_rectangles_and_skip_preoccupied_slots` (`toolbar_tests.rs:448`) |
| Writer P2 — explicit Arial | Resolved (reviewer's recheck agreed) | `externally_authored_arial_survives_import_save_and_pdf_export` |
| Writer P2 — complex-script formatting | Resolved under the reviewer's warning alternative. Script-specific modelling is not implemented, and the limit is recorded. | `complex_script_only_formatting_is_disclosed_and_does_not_restyle_latin_text`; the warning reaches the import dialog and the save preflight |
| Sheets P2 — oversized images | Fixed for embedded images in `cce0fbb9`. Path-only images are an open residual, and native measurement stays under PERF-01. | `an_oversized_decode_is_refused_before_pixels_are_allocated`, `the_decode_budget_is_inclusive_and_refuses_larger_declared_sizes` |

## Gate results

Run with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0` to stay within the sandbox disk allowance. These settings change build output only.

- Sheets (tree at `cce0fbb9`): `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets --locked -- -D warnings` clean; `cargo test --workspace --locked --no-fail-fast`: **1092 passed, 0 failed, 2 ignored** (the 2026-10-31 count of 1090, plus the two new regressions). The object-image module alone: 11 passed.
- Present (tree at `1bfb0b84`, unchanged in this pass): fmt clean; strict Clippy clean; `cargo test --workspace --locked --no-fail-fast`: **447 passed, 0 failed, 2 ignored**.
- Writer (tree at `1bfb0b84`, unchanged in this pass; the 2026-11-06 run): fmt clean; strict Clippy clean; **1315 passed, 0 failed, 5 ignored**.
- `loom-bootstrap/scripts/audit-governance.py`: PASS. `loom-bootstrap/scripts/documentation.py`: PASS (run after this pass's doc edits).

## Release evidence checked

GitHub Actions run `41185632779` (`Loom downloadable builds`, workflow dispatch) concluded `success` on commit `1b883d4b`. `release.yml` is unchanged from that commit to the PR head. The packaging step passes `--allow-unsigned` on every platform, so these artifacts are unsigned validation builds. Signing, SBOM and the support statement are open under GOV-08.

## Not done

- Native GPU input and frame measurement (PERF-01) and an RSS measurement of the first decode of a large image.
- Path-only oversized images (`Source::File`), which still use Slint's loader.
- Live Linux and Windows visual, accessibility (AT-SPI/UIA) and speech evidence; the Orca walkthroughs for Writer and Present.
- A fix for the Sheets Decimals row, which is clipped at the bottom of a 1280×800 window (found in the 2026-11-06 hover audit), and the visual matrix (viewport, theme, text scale, LTR/RTL) for it.
- Writer's full test binary in the 5.7 GiB Linux container (macOS suite is green).
- Independent acceptance review of any application.
- Windows and macOS testing beyond the recorded macOS captures; no Windows host was available.

## Merge

PR #21 is the merge vehicle for this pass. Its merge state is recorded on the PR, not here. Merging carries the repair pass into `main` and changes no product status.
