"""Regression checks for contradictory acceptance claims and the repair lock."""
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]


class GovernanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        manifest = tomllib.loads((ROOT / "loom-bootstrap/contracts/documentation.toml").read_text())
        names = [*manifest["roles"].values(), manifest["documents"]["index"],
                 *manifest["documents"]["archives"], "README.md",
                 "loom-bootstrap/contracts/documentation.toml",
                 "loom-bootstrap/contracts/workflow.toml",
                 "loom-bootstrap/scripts/documentation.py",
                 "loom-bootstrap/scripts/audit-governance.py"]
        for name in names:
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, target)
        self.use_office_v1_fixture()

    def truth_text(self):
        path = self.root / "docs/workflow/current.md"
        text = path.read_text(encoding="utf-8")
        match = re.search(
            r"<!-- CURRENT TRUTH START -->\s*(.*?)\s*<!-- CURRENT TRUTH END -->",
            text,
            re.DOTALL,
        )
        self.assertIsNotNone(match, "fixture must contain the marked current truth section")
        return match.group(1)

    def write_truth(self, truth):
        path = self.root / "docs/workflow/current.md"
        text = path.read_text(encoding="utf-8")
        updated, count = re.subn(
            r"(<!-- CURRENT TRUTH START -->\s*).*?(\s*<!-- CURRENT TRUTH END -->)",
            lambda match: match.group(1) + truth + match.group(2),
            text,
            count=1,
            flags=re.DOTALL,
        )
        self.assertEqual(count, 1)
        path.write_text(updated, encoding="utf-8")

    def replace(self, name, old, new):
        if name == "current_truth":
            text = self.truth_text()
            self.assertIn(old, text)
            self.write_truth(text.replace(old, new, 1))
            return
        path = self.root / name
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text)
        path.write_text(text.replace(old, new, 1), encoding="utf-8")

    def use_shared_repair_fixture(self):
        """Use the historical shared-recovery fixture for lock-regression tests."""
        workflow_path = self.root / "loom-bootstrap/contracts/workflow.toml"
        workflow = workflow_path.read_text(encoding="utf-8")
        workflow = re.sub(r'^phase = "[^"]+"$', 'phase = "audit-repair"', workflow, count=1, flags=re.MULTILINE)
        workflow = re.sub(r'^application_development_locked = (true|false)$',
                          'application_development_locked = true', workflow, count=1, flags=re.MULTILINE)
        workflow = re.sub(r'^consumer_imports_allowed = (true|false)$',
                          'consumer_imports_allowed = false', workflow, count=1, flags=re.MULTILINE)
        workflow = re.sub(r'^next_application = "[^"]+"$',
                          'next_application = "sheets"', workflow, count=1, flags=re.MULTILINE)
        workflow = re.sub(r'^active_repair = "[^"]+"\n', '', workflow, count=1, flags=re.MULTILINE)
        workflow = workflow.replace('consumer_imports_allowed = false\n',
                                    'consumer_imports_allowed = false\nactive_repair = "shared-recovery"\n', 1)
        workflow = re.sub(
            r'allowed_active_prefixes = \[.*?\n\]',
            '''allowed_active_prefixes = [
  ".github/",
  "AGENTS.md",
  "README.md",
  "loom-bootstrap/",
  "loom-core/crates/loom-ui/",
  "loom-core/crates/loom-desktop/",
  "loom-core/crates/loom-production/",
  "loom-core/crates/loom-storage/",
  "loom-design-bible/contracts/",
  "loom-design-bible/tokens/",
]''',
            workflow, count=1, flags=re.DOTALL,
        )
        workflow = workflow.replace('sheets = "IN_PROGRESS"', 'sheets = "LOCKED"', 1)
        workflow = workflow.replace('writer = "IN_PROGRESS"', 'writer = "LOCKED"', 1)
        workflow = workflow.replace('present = "IN_PROGRESS"', 'present = "LOCKED"', 1)
        workflow_path.write_text(workflow, encoding="utf-8")

        truth = self.truth_text()
        truth = re.sub(
            r'ACTIVE PHASE: [^\n]+\nFOUNDATION STATUS: [^\n]+\nSUITE STATUS: [^\n]+\n'
            r'APPLICATION DEVELOPMENT: [^\n]+\n(?:ACTIVE APPLICATIONS?: [^\n]+\n)?'
            r'(?:ACTIVE REPAIR: [^\n]+\n)?NEXT APPLICATION: [^\n]+',
            'ACTIVE PHASE: AUDIT-REPAIR\nFOUNDATION STATUS: ACCEPTED\nSUITE STATUS: ACCEPTANCE_BLOCKED\n'
            'APPLICATION DEVELOPMENT: LOCKED\nACTIVE REPAIR: SHARED-RECOVERY\nNEXT APPLICATION: SHEETS',
            truth, count=1,
        )
        truth = truth.replace('| 1 | Sheets | ACCEPTANCE_BLOCKED | IN_PROGRESS |',
                              '| 1 | Sheets | ACCEPTANCE_BLOCKED | LOCKED |', 1)
        truth = truth.replace('| 2 | Writer | ACCEPTANCE_BLOCKED | IN_PROGRESS |',
                              '| 2 | Writer | ACCEPTANCE_BLOCKED | LOCKED |', 1)
        truth = truth.replace('| 3 | Present | ACCEPTANCE_BLOCKED | IN_PROGRESS |',
                              '| 3 | Present | ACCEPTANCE_BLOCKED | LOCKED |', 1)
        self.write_truth(truth)

    def use_office_v1_fixture(self):
        """Use the owner-authorized three-app production work scope."""
        workflow_path = self.root / "loom-bootstrap/contracts/workflow.toml"
        workflow = workflow_path.read_text(encoding="utf-8")
        workflow = re.sub(r'^phase = "[^"]+"$', 'phase = "office-v1"', workflow,
                          count=1, flags=re.MULTILINE)
        workflow = re.sub(r'^next_application = "[^"]+"$',
                          'next_application = "photo"', workflow,
                          count=1, flags=re.MULTILINE)
        workflow = re.sub(r'^application_development_locked = (true|false)$',
                          'application_development_locked = false', workflow,
                          count=1, flags=re.MULTILINE)
        workflow = re.sub(r'^consumer_imports_allowed = (true|false)$',
                          'consumer_imports_allowed = true', workflow,
                          count=1, flags=re.MULTILINE)
        for app, status in (
            ("sheets", "IN_PROGRESS"),
            ("writer", "IN_PROGRESS"),
            ("present", "IN_PROGRESS"),
            ("photo", "LOCKED"),
            ("motion", "LOCKED"),
            ("video", "LOCKED"),
            ("studio", "LOCKED"),
            ("encode", "LOCKED"),
        ):
            workflow = re.sub(rf'^{app} = "[A-Z_]+"$',
                              f'{app} = "{status}"', workflow,
                              count=1, flags=re.MULTILINE)
        if '"loom-writer/"' not in workflow:
            workflow = workflow.replace(
                '  "loom-sheets/",\n',
                '  "loom-sheets/",\n  "loom-writer/",\n  "loom-present/",\n',
                1,
            )
        workflow_path.write_text(workflow, encoding="utf-8")

        truth = self.truth_text()
        updated, count = re.subn(
            r'ACTIVE PHASE: [^\n]+\nFOUNDATION STATUS: [^\n]+\nSUITE STATUS: [^\n]+\n'
            r'APPLICATION DEVELOPMENT: [^\n]+\nACTIVE APPLICATIONS?: [^\n]+\n'
            r'NEXT APPLICATION: [^\n]+',
            'ACTIVE PHASE: OFFICE-V1\nFOUNDATION STATUS: ACCEPTED\n'
            'SUITE STATUS: ACCEPTANCE_BLOCKED\nAPPLICATION DEVELOPMENT: UNLOCKED\n'
            'ACTIVE APPLICATIONS: SHEETS, WRITER, PRESENT\nNEXT APPLICATION: PHOTO',
            truth,
            count=1,
        )
        self.assertEqual(count, 1, "fixture must contain the active workflow gate")
        self.write_truth(updated)

    def run_audit(self):
        return subprocess.run(
            [sys.executable, str(self.root / "loom-bootstrap/scripts/audit-governance.py")],
            capture_output=True, text=True, timeout=15,
        )

    def rejects(self, expected):
        result = self.run_audit()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(expected, result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_recorded_office_v1_gate_is_valid_without_a_readiness_score(self):
        self.use_office_v1_fixture()
        result = self.run_audit()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_office_v1_cannot_accept_an_active_application(self):
        self.use_office_v1_fixture()
        self.replace("loom-bootstrap/contracts/workflow.toml",
                     'writer = "IN_PROGRESS"', 'writer = "ACCEPTED"')
        self.rejects("writer status must be IN_PROGRESS in office-v1 phase")

    def test_office_v1_cannot_unlock_a_later_application(self):
        self.use_office_v1_fixture()
        self.replace("loom-bootstrap/contracts/workflow.toml",
                     'photo = "LOCKED"', 'photo = "IN_PROGRESS"')
        self.rejects("application photo must remain LOCKED in office-v1 phase")

    def test_office_v1_cannot_promote_a_product_status(self):
        self.use_office_v1_fixture()
        self.replace("current_truth", "| 1 | Sheets | ACCEPTANCE_BLOCKED | IN_PROGRESS |",
                     "| 1 | Sheets | ACCEPTED | IN_PROGRESS |")
        self.rejects("Sheets product status must remain ACCEPTANCE_BLOCKED during office-v1")

    def test_office_v1_keeps_photo_as_the_next_application(self):
        self.use_office_v1_fixture()
        self.replace("loom-bootstrap/contracts/workflow.toml",
                     'next_application = "photo"', 'next_application = "writer"')
        self.rejects("office-v1 next_application must be photo")

    def test_office_v1_preserves_the_existing_foundation_consumer_list(self):
        self.use_office_v1_fixture()
        self.replace(
            "loom-bootstrap/contracts/workflow.toml",
            'existing_foundation_consumers = ["sheets", "writer", "present", "photo", "motion"]',
            'existing_foundation_consumers = ["sheets", "writer", "present"]',
        )
        self.rejects("office-v1 existing foundation consumer list changed")

    def test_office_v1_rejects_a_locked_app_prefix(self):
        self.use_office_v1_fixture()
        self.replace("loom-bootstrap/contracts/workflow.toml",
                     '  "loom-present/",', '  "loom-present/",\n  "loom-photo/",')
        self.rejects("office-v1 allowed_active_prefixes changed beyond approved scope")

    def test_older_supported_phases_remain_valid(self):
        workflow_path = self.root / "loom-bootstrap/contracts/workflow.toml"
        original_workflow = workflow_path.read_text(encoding="utf-8")
        apps = ("sheets", "writer", "present", "photo", "motion", "video", "studio", "encode")
        for phase in ("ui-foundation", "sheets", "writer", "present", "photo", "motion"):
            with self.subTest(phase=phase):
                workflow = re.sub(
                    r'^phase = "[^"]+"$', f'phase = "{phase}"', original_workflow,
                    count=1, flags=re.MULTILINE,
                )
                workflow = re.sub(r'^active_repair = "[^"]+"\n', "", workflow, count=1, flags=re.MULTILINE)
                display_phase = "UI FOUNDATION" if phase == "ui-foundation" else phase.upper()
                statuses = dict.fromkeys(apps, "LOCKED")
                if phase == "ui-foundation":
                    foundation = "ACCEPTANCE_BLOCKED"
                    development = "LOCKED"
                    workflow = re.sub(r'^application_development_locked = (true|false)$',
                                      "application_development_locked = true", workflow, count=1, flags=re.MULTILINE)
                    workflow = re.sub(r'^consumer_imports_allowed = (true|false)$',
                                      "consumer_imports_allowed = false", workflow, count=1, flags=re.MULTILINE)
                    workflow = workflow.replace('"ACCEPTED"', '"ACCEPTANCE_BLOCKED"')
                else:
                    foundation = "ACCEPTED"
                    development = "UNLOCKED"
                    next_app = apps[apps.index(phase) + 1]
                    workflow = re.sub(r'^next_application = "[^"]+"$',
                                      f'next_application = "{next_app}"', workflow, count=1, flags=re.MULTILINE)
                    workflow = re.sub(r'^application_development_locked = (true|false)$',
                                      "application_development_locked = false", workflow, count=1, flags=re.MULTILINE)
                    workflow = re.sub(r'^consumer_imports_allowed = (true|false)$',
                                      "consumer_imports_allowed = true", workflow, count=1, flags=re.MULTILINE)
                    for app in apps[:apps.index(phase)]:
                        workflow = re.sub(rf'^{app} = "(?:LOCKED|IN_PROGRESS|ACCEPTED)"$',
                                          f'{app} = "ACCEPTED"', workflow, count=1, flags=re.MULTILINE)
                        statuses[app] = "ACCEPTED"
                    workflow = re.sub(rf'^{phase} = "(?:LOCKED|IN_PROGRESS|ACCEPTED)"$',
                                      f'{phase} = "IN_PROGRESS"', workflow, count=1, flags=re.MULTILINE)
                    statuses[phase] = "IN_PROGRESS"
                truth = (f"# Loom — Current Truth\n\nACTIVE PHASE: {display_phase}\n"
                         f"FOUNDATION STATUS: {foundation}\n"
                         f"APPLICATION DEVELOPMENT: {development}\n")
                if phase != "ui-foundation":
                    truth += f"ACTIVE APPLICATION: {phase.upper()}\n"
                truth += "\n| Order | Application | Status |\n|---:|---|---|\n"
                truth += "".join(f"| {index} | {app.title()} | {statuses[app]} |\n"
                                 for index, app in enumerate(apps, 1))
                for app in apps:
                    workflow = re.sub(
                        rf'^{app} = "(?:LOCKED|IN_PROGRESS|ACCEPTED)"$',
                        f'{app} = "{statuses[app]}"', workflow,
                        count=1, flags=re.MULTILINE,
                    )
                workflow_path.write_text(workflow, encoding="utf-8")
                truth += "\n## Owner scope — 2026-10-27\n\nHistorical phase fixture.\n"
                self.write_truth(truth)
                result = self.run_audit()
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_table_cannot_promote_an_app_during_shared_repair(self):
        self.use_shared_repair_fixture()
        self.replace("current_truth", "| 1 | Sheets | ACCEPTANCE_BLOCKED | LOCKED |",
                     "| 1 | Sheets | ACCEPTED | LOCKED |")
        self.rejects("Sheets product status must remain ACCEPTANCE_BLOCKED")

    def test_unrelated_numbered_evidence_table_is_allowed(self):
        self.write_truth(self.truth_text() + "\n## Evidence inventory\n\n"
                         "| Order | Check | Input | Result |\n|---|---|---|---|\n"
                         "| 1 | Recovery | Disk | PASS |\n")
        result = self.run_audit()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_duplicate_application_table_is_rejected(self):
        self.use_shared_repair_fixture()
        truth = self.truth_text()
        start = truth.index("| Order | Application | Product status | Work status |")
        end = truth.index("\n\n", start)
        self.write_truth(truth + "\n" + truth[start:end] + "\n")
        self.rejects("exactly one ordered live application status table")

    def test_detailed_section_cannot_claim_a_different_status(self):
        self.use_shared_repair_fixture()
        self.replace("docs/workflow/capabilities.md", "### Present\n", "### Present\n\nStatus: `ACCEPTED`\n")
        self.rejects("Present section status disagrees with the live table")

    def test_workflow_cannot_unlock_an_app_during_shared_repair(self):
        self.use_shared_repair_fixture()
        self.replace("loom-bootstrap/contracts/workflow.toml", 'sheets = "LOCKED"',
                     'sheets = "IN_PROGRESS"')
        self.rejects("application sheets must remain LOCKED during shared-recovery")

    def test_active_phase_must_match_exactly(self):
        self.use_shared_repair_fixture()
        self.replace("current_truth", "ACTIVE PHASE: AUDIT-REPAIR", "ACTIVE PHASE: WRITER")
        self.rejects("docs/workflow/current.md current truth active phase disagrees with workflow")

    def test_live_gate_rejects_conflicting_duplicate_fields(self):
        self.use_shared_repair_fixture()
        original = self.truth_text()
        for field, expected, conflicting in (
            ("ACTIVE REPAIR", "SHARED-RECOVERY", "ENCODE"),
            ("NEXT APPLICATION", "SHEETS", "ENCODE"),
            ("APPLICATION DEVELOPMENT", "LOCKED", "UNLOCKED"),
            ("SUITE STATUS", "ACCEPTANCE_BLOCKED", "ACCEPTED"),
        ):
            with self.subTest(field=field):
                declaration = f"{field}: {expected}"
                self.write_truth(original.replace(
                    declaration, f"{declaration}\n{field}: {conflicting}", 1,
                ))
                self.rejects(f"docs/workflow/current.md current truth {field} must appear once and match workflow")

    def test_live_gate_cannot_be_supplied_only_by_incidental_prose(self):
        self.use_shared_repair_fixture()
        self.replace("current_truth", "ACTIVE REPAIR: SHARED-RECOVERY",
                     "Old example: ACTIVE REPAIR: SHARED-RECOVERY")
        self.rejects("docs/workflow/current.md current truth ACTIVE REPAIR must appear once and match workflow")

    def test_gate_values_allow_trailing_whitespace(self):
        self.use_shared_repair_fixture()
        fields = ("ACTIVE PHASE:", "FOUNDATION STATUS:", "APPLICATION DEVELOPMENT:",
                  "SUITE STATUS:", "ACTIVE REPAIR:", "NEXT APPLICATION:")
        self.write_truth("\n".join(line + "  " if line.startswith(fields) else line
                                    for line in self.truth_text().split("\n")))
        result = self.run_audit()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_duplicate_gate_values_still_fail_after_trimming(self):
        self.use_shared_repair_fixture()
        self.replace("current_truth", "ACTIVE REPAIR: SHARED-RECOVERY",
                     "ACTIVE REPAIR: SHARED-RECOVERY\nACTIVE REPAIR: SHARED-RECOVERY  ")
        self.rejects("docs/workflow/current.md current truth ACTIVE REPAIR must appear once and match workflow")

    def test_formatted_detailed_status_cannot_override_live_table(self):
        self.use_shared_repair_fixture()
        self.replace("docs/workflow/capabilities.md", "### Present\n",
                     "### Present\n\n**Status:** `ACCEPTED`\n")
        self.rejects("Present section status disagrees with the live table")

    def test_shared_repair_cannot_expand_product_prefixes(self):
        self.use_shared_repair_fixture()
        self.replace("loom-bootstrap/contracts/workflow.toml", "allowed_active_prefixes = [",
                     'allowed_active_prefixes = [\n  "loom-encode/",')
        self.rejects("shared-recovery allowed_active_prefixes changed")

    def test_shared_repair_can_narrow_optional_product_prefixes(self):
        self.use_shared_repair_fixture()
        self.replace("loom-bootstrap/contracts/workflow.toml",
                     '  "loom-core/crates/loom-ui/",\n', "")
        result = self.run_audit()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_unlisted_consumer_cannot_import_the_foundation(self):
        path = self.root / "loom-video/crates/loom-video-app/ui/app.slint"
        path.parent.mkdir(parents=True)
        path.write_text('import { LoomButton } from "foundation.slint";')
        self.rejects("locked application imports foundation")

    def test_nested_agent_authority_is_still_rejected(self):
        path = self.root / "loom-writer/AGENTS.md"
        path.parent.mkdir(parents=True)
        path.write_text("Conflicting instructions")
        self.rejects("nested agent authority is forbidden")

    def test_non_readme_markdown_must_be_consolidated(self):
        path = self.root / "loom-sheets/EXTRA.md"
        path.parent.mkdir(parents=True)
        path.write_text("Performance notes")
        self.rejects("Markdown path is not declared in documentation.toml")

    def test_owner_authorized_linux_input_skill_markdown_is_allowed(self):
        path = self.root / ".agents/skills/linux-desktop-input/SKILL.md"
        path.parent.mkdir(parents=True)
        path.write_text("Reusable Linux desktop interaction skill")
        result = self.run_audit()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_other_skill_markdown_is_still_rejected(self):
        path = self.root / ".agents/skills/another-skill/SKILL.md"
        path.parent.mkdir(parents=True)
        path.write_text("Not an owner-authorized exception")
        self.rejects("Markdown path is not declared in documentation.toml")

    def test_project_readmes_are_exempt_from_consolidation(self):
        path = self.root / "loom-sheets/README.md"
        path.parent.mkdir(parents=True)
        path.write_text("The application README stays here.")
        result = self.run_audit()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_marked_current_truth_section_is_required(self):
        path = self.root / "docs/workflow/current.md"
        path.write_text(path.read_text(encoding="utf-8").replace("<!-- CURRENT TRUTH START -->", "", 1), encoding="utf-8")
        self.rejects("docs/workflow/current.md must contain exactly one marked current truth section")

    def test_missing_workflow_reports_an_error_without_a_traceback(self):
        (self.root / "loom-bootstrap/contracts/workflow.toml").unlink()
        self.rejects("missing loom-bootstrap/contracts/workflow.toml")

    def test_missing_imported_rules_fail(self):
        (self.root / "docs/rules/design.md").unlink()
        self.rejects("missing declared documentation: docs/rules/design.md")

    def test_documentation_roles_cannot_redirect_the_live_ledger(self):
        self.replace("loom-bootstrap/contracts/documentation.toml",
                     'current = "docs/workflow/current.md"',
                     'current = "docs/archive/AGENTS-ARCHIVE.md"')
        self.rejects("documentation roles must match")

    def test_unreachable_imported_rules_fail(self):
        for path in self.root.rglob("*.md"):
            path.write_text(path.read_text().replace("platforms.md", "engineering.md"))
        self.rejects("document unreachable from AGENTS.md: docs/rules/platforms.md")

    def test_broken_reading_anchor_fails(self):
        self.replace("AGENTS.md", "current.md#current-truth", "current.md#nonexistent")
        self.rejects("broken documentation anchor")

    def test_competing_ledger_in_imported_document_fails(self):
        path = self.root / "docs/workflow/capabilities.md"
        path.write_text(path.read_text() + "\n| Order | Application | Product status |\n")
        self.rejects("competing live application status table")

    def test_truth_markers_cannot_move_to_an_archive(self):
        path = self.root / "docs/archive/AGENTS-ARCHIVE.md"
        path.write_text(path.read_text() + "\n<!-- CURRENT TRUTH START -->\n")
        self.rejects("current-truth marker outside canonical owner")

    def test_root_reading_budget_is_enforced(self):
        path = self.root / "AGENTS.md"
        path.write_text(path.read_text() + "x" * 5120)
        self.rejects("AGENTS.md entrypoint exceeds")


if __name__ == "__main__":
    unittest.main()
