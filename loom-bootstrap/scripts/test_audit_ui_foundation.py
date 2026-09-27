"""Regression tests for the UI foundation's token and runtime palette audit."""
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class UiFoundationPaletteAuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        files = (
            "loom-bootstrap/scripts/audit-ui-foundation.py",
            "loom-design-bible/contracts/ui-foundation.toml",
            "loom-design-bible/contracts/desktop-ui.toml",
            "loom-design-bible/tokens/loom.toml",
            "loom-core/crates/loom-ui/ui/foundation.slint",
            "loom-core/crates/loom-ui/ui/theme.slint",
        )
        for name in files:
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, target)
        shutil.copytree(
            ROOT / "loom-core/crates/loom-ui/ui/foundation",
            self.root / "loom-core/crates/loom-ui/ui/foundation",
        )
        shutil.copytree(
            ROOT / "loom-core/crates/loom-ui/baselines/foundation",
            self.root / "loom-core/crates/loom-ui/baselines/foundation",
        )

    def run_audit(self):
        return subprocess.run(
            [sys.executable, str(self.root / "loom-bootstrap/scripts/audit-ui-foundation.py")],
            cwd=self.root,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_dark_runtime_foreground_must_match_dark_tokens(self):
        theme_path = self.root / "loom-core/crates/loom-ui/ui/theme.slint"
        source = theme_path.read_text(encoding="utf-8")
        source = source.replace(
            "export global ThemeDark {\n",
            'export global ThemeDark {\n    in-out property <string> audit-note: "} palette: {";\n',
            1,
        )
        theme_path.write_text(source, encoding="utf-8")
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, f"strings must not close theme scopes: {baseline.stderr}")

        source = theme_path.read_text(encoding="utf-8")
        dark_start = source.index("export global ThemeDark {")
        dark_end = source.index("export global ThemeHighContrast {", dark_start)
        dark_source = source[dark_start:dark_end]
        backup_source = dark_source.replace(
            "export global ThemeDark {", "export global ThemeDarkBackup {", 1
        )
        source = source[:dark_start] + backup_source + source[dark_start:]
        dark_start = source.index("export global ThemeDark {")
        dark_end = source.index("export global ThemeHighContrast {", dark_start)
        dark_source = source[dark_start:dark_end]
        corrupted_dark, count = re.subn(
            r"(\baccent-ink:\s*)#[0-9a-fA-F]{6}(,)",
            r"\1#ffffff\2 // accent-ink: #000000,\n            /* } palette: { accent-ink: #000000 } */",
            dark_source,
            count=1,
            flags=re.DOTALL,
        )
        self.assertEqual(count, 1, "fixture changes only ThemeDark.accent-ink")
        theme_path.write_text(
            source[:dark_start] + corrupted_dark + source[dark_end:], encoding="utf-8"
        )

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "cross-theme color reuse must not pass")
        self.assertIn(
            "runtime theme.slint dark accent-ink value #ffffff does not match token #000000",
            result.stderr,
        )

        expression_dark, count = re.subn(
            r"(\baccent-ink:\s*)#[0-9a-fA-F]{6}(,)",
            r"\1#000000.with-alpha(0.0)\2",
            dark_source,
            count=1,
        )
        self.assertEqual(count, 1)
        theme_path.write_text(
            source[:dark_start] + expression_dark + source[dark_end:], encoding="utf-8"
        )
        expression_result = self.run_audit()
        self.assertNotEqual(expression_result.returncode, 0, "color expressions must not pass as literals")
        self.assertIn(
            "runtime theme.slint dark palette role accent-ink must be a six-digit opaque color literal",
            expression_result.stderr,
        )

        duplicate_dark, count = re.subn(
            r"(\baccent-ink:\s*#[0-9a-fA-F]{6})(,)",
            r"\1\2 accent-ink: #ffffff.with-alpha(0.0),",
            dark_source,
            count=1,
        )
        self.assertEqual(count, 1)
        theme_path.write_text(
            source[:dark_start] + duplicate_dark + source[dark_end:], encoding="utf-8"
        )
        duplicate_result = self.run_audit()
        self.assertNotEqual(duplicate_result.returncode, 0, "a malformed duplicate role must not be ignored")
        self.assertIn(
            "runtime theme.slint dark palette role accent-ink occurs more than once",
            duplicate_result.stderr,
        )

    def test_light_runtime_palette_must_come_from_exact_theme(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        theme_path = self.root / "loom-core/crates/loom-ui/ui/theme.slint"
        source = theme_path.read_text(encoding="utf-8")
        light_start = source.index("export global Theme {")
        light_end = source.index("export global ThemeDark {", light_start)
        light_source = source[light_start:light_end]
        backup_source = light_source.replace(
            "export global Theme {", "export global ThemeBackup {", 1
        )
        source = source[:light_start] + backup_source + source[light_start:]

        light_start = source.index("export global Theme {")
        light_end = source.index("export global ThemeDark {", light_start)
        light_source = source[light_start:light_end]
        corrupted_light, count = re.subn(
            r"(\baccent:\s*)#c4441a(\s*,)",
            r"\1#d95324\2",
            light_source,
            count=1,
        )
        self.assertEqual(count, 1)
        theme_path.write_text(
            source[:light_start] + corrupted_light + source[light_end:], encoding="utf-8"
        )

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "ThemeBackup must not satisfy the Light check")
        self.assertIn(
            "runtime theme.slint light accent value #d95324 does not match token #c4441a",
            result.stderr,
        )

    def test_unclosed_theme_scope_reports_auditor_error(self):
        theme_path = self.root / "loom-core/crates/loom-ui/ui/theme.slint"
        source = theme_path.read_text(encoding="utf-8")
        dark_start = source.index("export global ThemeDark {")
        dark_end = source.index("export global ThemeHighContrast {", dark_start)
        dark_source = source[dark_start:dark_end]
        closing_brace = dark_source.rfind("\n}")
        self.assertGreaterEqual(closing_brace, 0)
        malformed_dark = dark_source[: closing_brace + 1] + dark_source[closing_brace + 2 :]
        theme_path.write_text(
            source[:dark_start] + malformed_dark + source[dark_end:], encoding="utf-8"
        )

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unclosed Slint scope: dark", result.stderr)
        self.assertNotIn("NameError", result.stderr)

    def test_disabled_control_palette_is_checked_separately(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        theme_path = self.root / "loom-core/crates/loom-ui/ui/theme.slint"
        source = theme_path.read_text(encoding="utf-8")
        dark_start = source.index("export global ThemeDark {")
        dark_end = source.index("export global ThemeHighContrast {", dark_start)
        dark_source = source[dark_start:dark_end]
        corrupted_dark, count = re.subn(
            r"(\bink-disabled:\s*)#[0-9a-fA-F]{6}(,)",
            r"\1#000000\2",
            dark_source,
            count=1,
        )
        self.assertEqual(count, 1)
        theme_path.write_text(
            source[:dark_start] + corrupted_dark + source[dark_end:], encoding="utf-8"
        )

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "disabled runtime colors must match their tokens")
        self.assertIn(
            "runtime theme.slint dark ink-disabled value #000000 does not match token #8c8c94",
            result.stderr,
        )

    def test_disabled_ghost_surface_is_checked_for_transparent_buttons(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        theme_path = self.root / "loom-core/crates/loom-ui/ui/theme.slint"
        source = theme_path.read_text(encoding="utf-8")
        dark_start = source.index("export global ThemeDark {")
        dark_end = source.index("export global ThemeHighContrast {", dark_start)
        dark_source = source[dark_start:dark_end]
        corrupted_dark, count = re.subn(
            r"(\bsurface-raised:\s*)#[0-9a-fA-F]{6}(,)",
            r"\1#ffffff\2",
            dark_source,
            count=1,
        )
        self.assertEqual(count, 1)
        theme_path.write_text(
            source[:dark_start] + corrupted_dark + source[dark_end:], encoding="utf-8"
        )

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "transparent disabled buttons need their surface audited")
        self.assertIn(
            "runtime theme.slint dark surface-raised value #ffffff does not match token #24242a",
            result.stderr,
        )

    def test_contrast_token_cannot_lower_the_4_5_floor(self):
        token_path = self.root / "loom-design-bible/tokens/loom.toml"
        original = token_path.read_text(encoding="utf-8")
        self.assertIn("contrast-body = 4.5", original)
        for value in ("3.0", "nan", "inf", "-inf"):
            with self.subTest(value=value):
                token_path.write_text(
                    original.replace("contrast-body = 4.5", f"contrast-body = {value}", 1),
                    encoding="utf-8",
                )
                result = self.run_audit()
                self.assertNotEqual(
                    result.returncode, 0, "lowered or non-finite token values must not lower the audit floor"
                )
                self.assertIn(
                    "loom.toml a11y.contrast-body must be a finite number at least 4.5:1",
                    result.stderr,
                )

        token_path.write_text(
            original.replace("contrast-body = 4.5", "contrast-body = 7.0", 1),
            encoding="utf-8",
        )
        raised = self.run_audit()
        self.assertNotEqual(raised.returncode, 0)
        self.assertIn("light accent/accent-ink contrast", raised.stderr)
        self.assertIn("requires 7.0:1", raised.stderr)


if __name__ == "__main__":
    unittest.main()
