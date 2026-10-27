"""App accents must retain contract parity and accessible interaction colours."""
from __future__ import annotations

import re

import pytest

import test_audit_ui_foundation as fixtures


@pytest.fixture
def audit_fixture():
    fixture = fixtures.UiFoundationPaletteAuditTests()
    fixture.setUp()
    try:
        yield fixture
    finally:
        fixture.doCleanups()


def test_application_accent_runtime_drift_fails(audit_fixture) -> None:
    source = audit_fixture.root / "loom-core/crates/loom-ui/ui/app-accents.slint"
    original = source.read_text()
    assert "accent: #6241ad," in original
    source.write_text(original.replace("accent: #6241ad,", "accent: #0071e3,", 1))
    result = audit_fixture.run_audit()
    assert result.returncode != 0
    assert "writer light accent" in result.stderr


def test_application_accent_contract_drift_fails(audit_fixture) -> None:
    contract = audit_fixture.root / "loom-design-bible/contracts/desktop-ui.toml"
    original = contract.read_text()
    assert 'accent = "#6241AD"' in original
    contract.write_text(original.replace('accent = "#6241AD"', 'accent = "#0071E3"', 1))
    result = audit_fixture.run_audit()
    assert result.returncode != 0
    assert "writer light accent" in result.stderr


def test_application_selection_contrast_cannot_be_weakened(audit_fixture) -> None:
    tokens = audit_fixture.root / "loom-design-bible/tokens/loom.toml"
    original = tokens.read_text()
    changed, count = re.subn(
        r'(\[application-accent\.writer\.light\][\s\S]*?\nselection = )"#[0-9A-F]+"',
        r'\1"#FFFFFF"',
        original,
        count=1,
    )
    assert count == 1
    tokens.write_text(changed)
    result = audit_fixture.run_audit()
    assert result.returncode != 0
    assert "writer light selection/accent-ink contrast" in result.stderr
