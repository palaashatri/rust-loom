# Test all

Run the suite gates from `loom-bootstrap/`:

```bash
bash scripts/fmt-all.sh
bash scripts/clippy-all.sh
bash scripts/test-all.sh
```

The complete extracted-package verification covers all 11 Cargo workspaces and
276 passing tests:

| Workspace | Tests |
|---|---:|
| `loom-core` | 84 |
| `loom-writer` | 20 |
| `loom-sheets` | 18 |
| `loom-present` | 5 |
| `loom-photo` | 4 |
| `loom-motion` | 5 |
| `loom-video` | 4 |
| `loom-studio` | 4 |
| `loom-encode` | 3 |
| `loom-vision` | 72 |
| `loom-plugin-sdk` | 57 |

The extracted-package logs are in `loom-bootstrap/.work/verify-test-loom-*.log`.
A workspace that is absent or returns zero passing tests is incomplete/failing;
the summary cannot report PASS for it. A fresh Docker CI test phase also ran
against the current worktree, but it was interrupted during photo testing
after core, Writer, Sheets, and Present had passed; it is not claimed as a
completed aggregate run.

## Visual regression

```bash
bash scripts/visual-qa-all.sh
```

The current Docker run captured all 16 app/theme images and compared all 16
application baselines with zero diffs, zero missing baselines, zero size
mismatches, and zero capture/comparison failures. The default light/dark gate
is **PASS**. The full design-bible matrix is not run by this harness, and the
new baselines came from the same reviewed capture set, so this is a
capture/baseline consistency result rather than independent historical
regression evidence.

## Offline and Docker checks

```bash
bash scripts/offline-test.sh --mode-a
bash scripts/docker-offline-test.sh
bash scripts/docker-visual-qa.sh
```

These are reported as evidence only after the command completes. The visual
Docker image is pinned to the CI image's already-installed Xvfb/software
renderer dependencies.
