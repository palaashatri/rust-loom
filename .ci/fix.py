from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}, found {count}: {old!r}")
    file.write_text(text.replace(old, new), encoding="utf-8")


replace_once(
    "loom-photo/crates/loom-photo-core/src/lib.rs",
    """            for channel in 0..3 {\n                let value =\n                    pixel[channel] as f32 * alpha + background[channel] as f32 * (1.0 - alpha);\n                output.push(value.round().clamp(0.0, 255.0) as u8);\n            }""",
    """            for (source, backdrop) in pixel.iter().take(3).zip(background.iter()) {\n                let value = *source as f32 * alpha + *backdrop as f32 * (1.0 - alpha);\n                output.push(value.round().clamp(0.0, 255.0) as u8);\n            }""",
)

for path in (
    "loom-studio/crates/loom-studio-app/ui/app.slint",
    "loom-video/crates/loom-video-app/ui/app.slint",
):
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    text = text.replace("index % 2", "Math.mod(index, 2)")
    text = text.replace("Math.floor(seconds) % 60", "Math.mod(Math.floor(seconds), 60)")
    if " % " in text:
        raise SystemExit(f"unsupported modulo expression remains in {path}")
    file.write_text(text, encoding="utf-8")

replace_once(
    "loom-core/crates/loom-ui/ui/smoke.slint",
    'import { Theme } from "theme.slint";\n',
    'import { Theme } from "theme.slint";\nexport { Theme } from "theme.slint";\n',
)

Path(".github/workflows/ci.yml").write_text(
    '''name: Loom CI

on:
  pull_request:
  push:
    branches: [main, cline-implementation]
  workflow_dispatch:

permissions:
  contents: read

concurrency:
  group: loom-ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  contracts:
    runs-on: ubuntu-24.04
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v4
      - name: Audit UI/Rust contracts
        run: python3 loom-bootstrap/scripts/audit-contracts.py

  workspace:
    name: ${{ matrix.workspace }}
    runs-on: ubuntu-24.04
    timeout-minutes: 60
    strategy:
      fail-fast: false
      max-parallel: 6
      matrix:
        workspace:
          - loom-core
          - loom-writer
          - loom-sheets
          - loom-present
          - loom-photo
          - loom-motion
          - loom-video
          - loom-studio
          - loom-encode
          - loom-vision
          - loom-plugin-sdk
    steps:
      - uses: actions/checkout@v4
      - name: Install native build dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y --no-install-recommends \\
            build-essential pkg-config libasound2-dev \\
            libfontconfig1-dev libx11-dev libxkbcommon-dev libwayland-dev \\
            libgl1-mesa-dev libglu1-mesa-dev
      - name: Select Rust stable with lint and format components
        run: |
          rustup toolchain install stable --profile minimal --component rustfmt --component clippy
          rustup default stable
          rustc --version
          cargo --version
      - name: Cache Cargo outputs
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: ${{ matrix.workspace }}
          shared-key: loom-${{ matrix.workspace }}
      - name: Check formatting
        working-directory: ${{ matrix.workspace }}
        run: cargo fmt --all --check
      - name: Lint
        working-directory: ${{ matrix.workspace }}
        run: cargo clippy --workspace --all-targets --all-features -- -D warnings
      - name: Test
        working-directory: ${{ matrix.workspace }}
        run: cargo test --workspace
      - name: Build release
        working-directory: ${{ matrix.workspace }}
        run: cargo build --workspace --release

  visual-smoke:
    needs: [contracts, workspace]
    runs-on: ubuntu-24.04
    timeout-minutes: 120
    steps:
      - uses: actions/checkout@v4
      - name: Build visual-QA image
        run: docker compose -f loom-bootstrap/docker/compose.yaml build visual
      - name: Build applications and capture theme matrix
        run: >-
          docker compose -f loom-bootstrap/docker/compose.yaml run --rm visual
          bash -lc "scripts/build-all.sh --release && xvfb-run -a -s '-screen 0 1280x800x24' scripts/visual-smoke-matrix.sh"
      - name: Upload visual evidence
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: loom-theme-matrix
          if-no-files-found: warn
          retention-days: 14
          path: |
            loom-bootstrap/.work/theme-matrix/
            loom-bootstrap/.work/theme-matrix-report.md
            loom-bootstrap/.work/theme-*.log
''',
    encoding="utf-8",
)
