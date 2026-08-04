# Loom — Visual QA All

Deterministic native visual QA for all eight applications. All evidence below
was produced from the real release binaries (no mock or doctored captures).

## Evidence layers

1. **Native UI matrix** — `loom-bootstrap/scripts/native-ui-matrix.py`
   - 8 apps x 3 sizes (1024x720, 1440x900, 1920x1200) x 3 themes
     (light, dark, high-contrast)
   - plus per-app sample-open capture and open-palette capture
   - rejects byte-identical palette captures (real open state required)
   - result: `passed: true` (2026-08-04, fresh release binaries)
   - report: `loom-bootstrap/.work/evidence/ui/native-ui-matrix.json` + PNGs

2. **Theme smoke matrix** — `loom-bootstrap/scripts/visual-smoke-matrix.sh`
   - 8/8 apps captured in light, dark, high-contrast
   - requires all three theme captures byte-distinct (rejects fake theme
     switching)
   - result: 8/8 PASS, all themes distinct (2026-08-04)

3. **Recorded keyboard journeys** — `loom-core/crates/loom-test-support`
   - per-app command palette journeys: open -> type query -> narrow list ->
     move selection -> invoke -> dismiss, dispatched through the real Slint
     input pipeline with a screenshot per step
   - 8/8 PASS: writer `ex`, sheets `format`, present `template`, studio
     `workspace`, encode `queue`, photo `layer`, motion `frame`, video `clip`
   - evidence: `loom-bootstrap/.work/evidence/journeys/<app>/` (JSON + PNGs)

4. **Functional matrix** — `loom-bootstrap/scripts/native-functional-matrix.py`
   - CLI create/validate/export journeys per app, package validation,
     signature checks on outputs
   - result: passed (8/8 applications, 2026-08-04)

5. **Baseline review** — `docs/visual-qa-baseline-review.md` documents the
   earlier Docker capture-set review (16 light/dark captures inspected
   manually for window, nonblank content, layout, selection state).

## Commands

```bash
bash loom-bootstrap/scripts/visual-smoke-matrix.sh          # theme smoke matrix
python3 loom-bootstrap/scripts/native-ui-matrix.py \
  --root . --output loom-bootstrap/.work/evidence/ui --platform linux
python3 loom-bootstrap/scripts/native-functional-matrix.py \
  --root . --output loom-bootstrap/.work/evidence --platform linux
bash loom-bootstrap/scripts/generate-status-report.sh       # VERIFICATION_REPORT.md
```

## Not covered (honest gaps)

- Golden-baseline regression diffing (requires pinned renderer baselines;
  the design-bible baseline directory is not yet populated).
- Component-state, error-state, RTL, text-scale, and reduced-motion capture
  sets (designed in `loom-design-bible/VISUAL_QA.md`, not yet executed).
- Locale stress captures.
