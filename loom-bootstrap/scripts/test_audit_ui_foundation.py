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
            "loom-sheets/crates/loom-sheets-app/ui/objects.slint",
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
            r"\1#000000\2 // accent-ink: #ffffff,\n            /* } palette: { accent-ink: #ffffff } */",
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
            "runtime theme.slint dark accent-ink value #000000 does not match token #ffffff",
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
            r"(\baccent:\s*)#0071e3(\s*,)",
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
            "runtime theme.slint light accent value #d95324 does not match token #0071e3",
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
            "runtime theme.slint dark ink-disabled value #000000 does not match token #9a9aa2",
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
            "runtime theme.slint dark surface-raised value #ffffff does not match token #2c2c30",
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

    def test_shape_label_audit_rejects_theme_ink_for_pastel_fills(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        objects_path = self.root / "loom-sheets/crates/loom-sheets-app/ui/objects.slint"
        source = objects_path.read_text(encoding="utf-8")
        corrupted, count = re.subn(
            r"(?<![\w-])color:\s*[^;]+;",
            "color: Theme.palette().ink;",
            source,
            count=1,
        )
        self.assertEqual(count, 1, "fixture changes the shape label foreground only")
        objects_path.write_text(corrupted, encoding="utf-8")

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "dark theme ink must not be used on every pastel fill")
        self.assertIn("shape label foreground", result.stderr.lower())

    def test_shape_label_contrast_is_checked_for_each_theme(self):
        token_path = self.root / "loom-design-bible/tokens/loom.toml"
        theme_path = self.root / "loom-core/crates/loom-ui/ui/theme.slint"
        original_tokens = token_path.read_text(encoding="utf-8")
        original_theme = theme_path.read_text(encoding="utf-8")
        theme_ranges = {
            "light": ("export global Theme {", "export global ThemeDark {"),
            "dark": ("export global ThemeDark {", "export global ThemeHighContrast {"),
            "high-contrast": ("export global ThemeHighContrast {", None),
        }

        for theme, (opening, next_opening) in theme_ranges.items():
            with self.subTest(theme=theme):
                token_path.write_text(original_tokens, encoding="utf-8")
                theme_path.write_text(original_theme, encoding="utf-8")
                token_start = original_tokens.index(f"[palette.{theme}]")
                token_end = original_tokens.find("\n[", token_start + 1)
                if token_end < 0:
                    token_end = len(original_tokens)
                token_block = original_tokens[token_start:token_end]
                changed_tokens, token_count = re.subn(
                    r'(\bpaper-ink\s*=\s*)"#[0-9a-fA-F]{6}"',
                    r'\1"#888888"',
                    token_block,
                    count=1,
                )
                self.assertEqual(token_count, 1, f"fixture changes {theme} paper ink token")
                token_path.write_text(
                    original_tokens[:token_start] + changed_tokens + original_tokens[token_end:],
                    encoding="utf-8",
                )

                theme_source = theme_path.read_text(encoding="utf-8")
                theme_start = theme_source.index(opening)
                theme_end = (
                    theme_source.index(next_opening, theme_start + len(opening))
                    if next_opening
                    else len(theme_source)
                )
                theme_block = theme_source[theme_start:theme_end]
                changed_theme, theme_count = re.subn(
                    r"(\bpaper-ink:\s*)#[0-9a-fA-F]{6}(?=\s*,)",
                    r"\1#888888",
                    theme_block,
                    count=1,
                )
                self.assertEqual(theme_count, 1, f"fixture changes {theme} runtime paper ink")
                theme_path.write_text(
                    theme_source[:theme_start] + changed_theme + theme_source[theme_end:],
                    encoding="utf-8",
                )

                result = self.run_audit()
                self.assertNotEqual(
                    result.returncode,
                    0,
                    f"low-contrast shape label ink must fail in the {theme} theme",
                )
                self.assertIn("shape fill", result.stderr.lower())
                self.assertIn(theme, result.stderr.lower())

    def test_runtime_ink_must_match_the_dark_token(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        theme_path = self.root / "loom-core/crates/loom-ui/ui/theme.slint"
        source = theme_path.read_text(encoding="utf-8")
        dark_start = source.index("export global ThemeDark {")
        dark_end = source.index("export global ThemeHighContrast {", dark_start)
        dark_source = source[dark_start:dark_end]
        corrupted_dark, count = re.subn(
            r"(?<![\w-])(ink\s*:\s*)#[0-9a-fA-F]{6}(?=\s*,)",
            r"\1#24242a",
            dark_source,
            count=1,
        )
        self.assertEqual(count, 1, "fixture changes the runtime ink role only")
        theme_path.write_text(
            source[:dark_start] + corrupted_dark + source[dark_end:], encoding="utf-8"
        )

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "runtime ink must be tied to the approved token")
        self.assertIn(
            "runtime theme.slint dark ink value #24242a does not match token #f5f5f7",
            result.stderr,
        )

    def test_unfilled_shape_contrast_uses_runtime_ink(self):
        token_path = self.root / "loom-design-bible/tokens/loom.toml"
        theme_path = self.root / "loom-core/crates/loom-ui/ui/theme.slint"
        token_source = token_path.read_text(encoding="utf-8")
        token_block_start = token_source.index("[palette.dark]")
        token_block_end = token_source.find("\n[", token_block_start + 1)
        token_block = token_source[token_block_start:token_block_end]
        changed_token_block, token_count = re.subn(
            r'(\bink\s*=\s*)"#[0-9a-fA-F]{6}"',
            r'\1"#24242a"',
            token_block,
            count=1,
        )
        self.assertEqual(token_count, 1, "fixture changes dark ink token only")
        token_path.write_text(
            token_source[:token_block_start]
            + changed_token_block
            + token_source[token_block_end:],
            encoding="utf-8",
        )

        theme_source = theme_path.read_text(encoding="utf-8")
        dark_start = theme_source.index("export global ThemeDark {")
        dark_end = theme_source.index("export global ThemeHighContrast {", dark_start)
        dark_block = theme_source[dark_start:dark_end]
        changed_theme, theme_count = re.subn(
            r"(?<![\w-])(ink\s*:\s*)#[0-9a-fA-F]{6}(?=\s*,)",
            r"\1#24242a",
            dark_block,
            count=1,
        )
        self.assertEqual(theme_count, 1, "fixture aligns runtime ink with low-contrast token")
        theme_path.write_text(
            theme_source[:dark_start] + changed_theme + theme_source[dark_end:],
            encoding="utf-8",
        )

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "unfilled labels need readable dark ink")
        self.assertIn("Sheets unfilled shape in dark ink contrast", result.stderr)

    def test_shape_background_audit_rejects_duplicate_unsafe_fill_branch(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        objects_path = self.root / "loom-sheets/crates/loom-sheets-app/ui/objects.slint"
        source = objects_path.read_text(encoding="utf-8")
        original = "root.objects[idx].fill == 0 ? #FECACA"
        unsafe_duplicate = (
            "root.objects[idx].fill == 0 ? #18181B\n"
            "                : root.objects[idx].fill == 0 ? #FECACA"
        )
        self.assertIn(original, source)
        objects_path.write_text(source.replace(original, unsafe_duplicate, 1), encoding="utf-8")

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "duplicate fill branches must not mask the rendered color")
        self.assertIn("fill index 0 occurs 2 times", result.stderr)

    def test_shape_background_audit_rejects_unsafe_fallback(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        objects_path = self.root / "loom-sheets/crates/loom-sheets-app/ui/objects.slint"
        source = objects_path.read_text(encoding="utf-8")
        self.assertIn(": Theme.palette().surface-raised)", source)
        objects_path.write_text(
            source.replace(
                ": Theme.palette().surface-raised)",
                ": Theme.palette().ink)",
                1,
            ),
            encoding="utf-8",
        )

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "shape fallback must use the audited surface")
        self.assertIn("shape background must preserve its approved fill mapping and fallbacks", result.stderr)

    def test_shape_label_audit_rejects_a_safe_decoy_before_the_real_label(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        objects_path = self.root / "loom-sheets/crates/loom-sheets-app/ui/objects.slint"
        source = objects_path.read_text(encoding="utf-8")
        label_foreground = (
            "root.objects[idx].fill >= 0 && root.objects[idx].fill <= 6 "
            "? Theme.palette().paper-ink : Theme.palette().ink"
        )
        decoy = (
            '        if root.objects[idx].kind == "shape" && false : Text {\n'
            f"            color: {label_foreground};\n"
            "        }\n\n"
        )
        actual_label = '        if root.objects[idx].kind == "shape" : Text {\n'
        self.assertIn(actual_label, source)
        source = source.replace(actual_label, decoy + actual_label, 1)
        actual_label_start = source.rfind(actual_label)
        actual_foreground_start = source.index("color:", actual_label_start)
        actual_foreground_end = source.index(";", actual_foreground_start)
        source = (
            source[:actual_foreground_start]
            + "color: Theme.palette().ink"
            + source[actual_foreground_end:]
        )
        objects_path.write_text(source, encoding="utf-8")

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "an inert safe Text node must not hide the real label")
        self.assertIn("must define exactly one Text child", result.stderr)

    def test_shape_label_audit_requires_the_shape_kind_condition(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        objects_path = self.root / "loom-sheets/crates/loom-sheets-app/ui/objects.slint"
        source = objects_path.read_text(encoding="utf-8")
        original = 'if root.objects[idx].kind == "shape" : Text {'
        self.assertIn(original, source)
        objects_path.write_text(
            source.replace(original, 'if root.objects[idx].kind != "shape" : Text {', 1),
            encoding="utf-8",
        )

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "shape labels must not be hidden behind a non-shape condition")
        self.assertIn("must be conditioned on kind == shape", result.stderr)

        commented = source.replace(
            original,
            'if root.objects[idx].kind /* valid shape guard */ == "shape" : Text {',
            1,
        )
        objects_path.write_text(commented, encoding="utf-8")
        comment_result = self.run_audit()
        self.assertEqual(comment_result.returncode, 0, "valid comments in the shape guard must remain accepted")

    def test_shape_label_audit_rejects_comment_tokens_inside_kind_string(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        objects_path = self.root / "loom-sheets/crates/loom-sheets-app/ui/objects.slint"
        source = objects_path.read_text(encoding="utf-8")
        original = 'if root.objects[idx].kind == "shape" : Text {'
        self.assertIn(original, source)
        objects_path.write_text(
            source.replace(original, 'if root.objects[idx].kind == "shape/*hidden*/" : Text {', 1),
            encoding="utf-8",
        )

        result = self.run_audit()
        self.assertNotEqual(
            result.returncode,
            0,
            "comment markers inside a kind string must not make a different kind pass",
        )
        self.assertIn("must be conditioned on kind == shape", result.stderr)

    def test_shape_audit_rejects_a_second_unsafe_object_repeater(self):
        baseline = self.run_audit()
        self.assertEqual(baseline.returncode, 0, baseline.stderr)

        objects_path = self.root / "loom-sheets/crates/loom-sheets-app/ui/objects.slint"
        source = objects_path.read_text(encoding="utf-8")
        component_end = source.rfind("\n}")
        self.assertGreater(component_end, 0, "fixture locates the component's closing brace")
        unsafe_repeater = '''

    for idx in root.objects.length : Rectangle {
        x: root.objects[idx].x;
        y: root.objects[idx].y;
        width: max(80px, root.objects[idx].width * 1px);
        height: max(48px, root.objects[idx].height * 1px);
        if root.objects[idx].kind == "shape" : Text {
            text: root.objects[idx].label;
            color: Theme.palette().ink;
        }
    }
'''
        objects_path.write_text(
            source[:component_end] + unsafe_repeater + source[component_end:],
            encoding="utf-8",
        )

        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0, "the real unsafe second repeater must not evade the audit")
        self.assertIn("must define exactly one object Rectangle repeater", result.stderr)


if __name__ == "__main__":
    unittest.main()
