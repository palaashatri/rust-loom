# Loom verification report — package

This report is regenerated after packaging. It must describe the archive and
the extracted-copy checks from the same run; older archive checksums or partial
workspace claims must not be retained.

## Required evidence

1. `scripts/package.sh` creates a deterministic archive and checksum while
   excluding `target/`, `.work/`, `.git/`, OS junk, and Python caches.
2. `scripts/verify-package.sh` extracts the archive, runs the environment check,
   parses every extracted Cargo workspace, and runs `cargo test --locked
   --offline --workspace` for each one into a temporary target directory.
3. `scripts/run-apps.sh` and `scripts/visual-qa-all.sh` are run against the
   current built binaries. Visual QA is a pass only when every required
   baseline exists and every comparison is within tolerance.
4. Docker CI, offline, and visual results are listed only when their commands
   complete; image-build failures remain explicit blockers.

The final archive path, byte size, SHA-256, workspace counts, test totals,
smoke result, visual result, and Docker result belong below this line and must
be copied from the final command output.

## Final evidence

- Archive: `/Users/palaashatri/Code/loom/rust-loom/Loom-Complete.zip`
- Archive contents: 309 files
- Archive size: 1,054,365 bytes
- SHA-256: `ff64e72da035feb909ec5cffc88bcc3e4fadc28bfb08b4d76728b25028f5086f`
- Host Cargo gates: fmt 11/11, clippy 11/11 with 0 Loom-crate issues, build
  11/11, and tests 11/11 with 250 tests passed
- Application smoke: 8/8 binaries exited 0
- Extracted package verification: 11/11 workspaces passed metadata and tests;
  command exit 0; `verify-target` and `verify-extract` cleanup passed
- Visual QA: 16 captures, 4 comparisons, 0 diffs, 12 missing baselines;
  result **INCOMPLETE/FAIL**
- Docker/offline verification: not rerun in this final host evidence pass

## Current visual limitation

The current strict visual run has 16 fresh captures, 4 existing baseline
comparisons with zero diffs, and 12 missing required baselines. It is therefore
**INCOMPLETE/FAIL**. The screenshots are preserved in
`loom-bootstrap/.work/screenshots/` for inspection; they are not silently
auto-approved as baselines.
