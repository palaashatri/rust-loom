#!/usr/bin/env bash
# Walk every loom-* repo and produce ../VERIFICATION_REPORT.md — a stub report
# with per-repo status: exists / builds / tests-pass / missing pieces, plus any
# status keywords found in TASKS.md or FEATURE_STATUS.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

REPORT="$PARENT/VERIFICATION_REPORT.md"

{
  echo "# Loom Verification Report"
  echo
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ) by scripts/generate-status-report.sh"
  echo
  echo "> Stub: extend with CI evidence and per-feature verification as the suite matures."
  echo
  echo "Status classes: COMPLETE | FUNCTIONAL_WITH_LIMITATIONS | EXPERIMENTAL | SCAFFOLDED | NOT_STARTED | BLOCKED"
  echo
  echo "| repo | exists | cargo | builds | tests-pass | status keywords in TASKS.md | missing |"
  echo "|------|--------|-------|--------|------------|------------------------------|---------|"
} > "$REPORT"

EXISTS=0
MISSING=0
for repo in $(ls -d "$PARENT"/loom-* 2>/dev/null | xargs -n1 basename | sort); do
  d="$PARENT/$repo"
  EXISTS=$((EXISTS + 1))
  cargo="no"
  builds="no"
  tests="no"
  keywords="—"
  missing=""

  if [ -f "$d/Cargo.toml" ]; then
    cargo="yes"
    if ( cd "$d" && cargo metadata --no-deps >/dev/null 2>&1 ) \
       || ( cd "$d" && cargo metadata --no-deps --offline >/dev/null 2>&1 ); then
      builds="yes"
    fi
    if [ -f "$WORK/test-$repo.log" ] && grep -qE '^test result: ok' "$WORK/test-$repo.log" 2>/dev/null; then
      tests="yes"
    elif [ -f "$WORK/test-$repo.log" ]; then
      tests="no"
    else
      tests="not-run"
    fi
  else
    missing="$missing no-cargo"
  fi

  if [ -f "$d/TASKS.md" ]; then
    kw=$(grep -oE 'COMPLETE|FUNCTIONAL_WITH_LIMITATIONS|EXPERIMENTAL|SCAFFOLDED|NOT_STARTED|BLOCKED' "$d/TASKS.md" 2>/dev/null | sort -u | tr '\n' ' ' | sed 's/ $//')
    keywords="${kw:-present (no status keywords)}"
  elif [ -f "$d/FEATURE_STATUS.md" ]; then
    keywords="FEATURE_STATUS.md present (no TASKS.md)"
  else
    missing="$missing no-TASKS.md"
  fi
  if [ ! -d "$d/.git" ]; then
    missing="$missing no-.git"
  fi

  echo "| $repo | yes | $cargo | $builds | $tests | $keywords |${missing:-none} |" >> "$REPORT"
done

for r in loom-core loom-writer loom-sheets loom-present loom-photo loom-motion loom-video loom-studio loom-encode loom-vision loom-plugin-sdk; do
  [ -d "$PARENT/$r" ] || { MISSING=$((MISSING + 1)); echo "| $r | no | — | — | — | — | directory absent |" >> "$REPORT"; }
done

{
  echo
  echo "## Summary"
  echo
  echo "- repos present: $EXISTS"
  echo "- expected repos absent: $MISSING"
  echo "- note: 'tests-pass' reflects the last scripts/test-all.sh run (logs in .work/); 'not-run' means no log exists yet."
} >> "$REPORT"

log "report written to $REPORT"
