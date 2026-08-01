# Test all

Run the suite gates from `loom-bootstrap/`:

```bash
bash scripts/fmt-all.sh
bash scripts/clippy-all.sh
bash scripts/test-all.sh
```

The latest host evidence covers all 11 Cargo workspaces and 250 passing tests:

| Workspace | Tests |
|---|---:|
| `loom-core` | 84 |
| `loom-writer` | 6 |
| `loom-sheets` | 12 |
| `loom-present` | 5 |
| `loom-photo` | 3 |
| `loom-motion` | 3 |
| `loom-video` | 3 |
| `loom-studio` | 3 |
| `loom-encode` | 2 |
| `loom-vision` | 72 |
| `loom-plugin-sdk` | 57 |

The orchestration scripts retain full logs in `loom-bootstrap/.work/`. A
workspace that is absent or returns zero passing tests is incomplete/failing;
the summary cannot report PASS for it.

## Visual regression

```bash
bash scripts/visual-qa-all.sh
```

The current run captured all 16 app/theme images and compared the 4 available
writer/sheets baselines with zero diffs. Twelve required baselines are absent,
so the overall visual result remains **INCOMPLETE/FAIL**.

## Offline and Docker checks

```bash
bash scripts/offline-test.sh --mode-a
bash scripts/docker-offline-test.sh
bash scripts/docker-visual-qa.sh
```

These are reported as evidence only after the command completes. The visual
Docker image is pinned to the CI image's already-installed Xvfb/software
renderer dependencies.
