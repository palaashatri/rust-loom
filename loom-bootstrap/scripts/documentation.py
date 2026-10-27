"""Load Loom's declared document owners and verify their local reading routes."""
from __future__ import annotations

import re
import tomllib
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROLE_PATHS = {
    "entrypoint": "AGENTS.md",
    "engineering": "docs/rules/engineering.md",
    "workflow": "docs/rules/workflow.md",
    "design": "docs/rules/design.md",
    "platforms": "docs/rules/platforms.md",
    "current": "docs/workflow/current.md",
    "writer": "docs/workflow/writer.md",
    "capabilities": "docs/workflow/capabilities.md",
    "repairs": "docs/workflow/repairs.md",
    "debt": "docs/workflow/governance-debt.md",
    "audit": "docs/audits/uiux-2026-10-27.md",
}
EXCEPTIONS = {Path(".agents/skills/linux-desktop-input/SKILL.md")}
TRUTH_BEGIN = "<!-- CURRENT TRUTH START -->"
TRUTH_END = "<!-- CURRENT TRUTH END -->"


def markdown_anchors(text: str) -> set[str]:
    """GitHub-style heading IDs, including duplicates and explicit HTML IDs."""
    text = re.sub(r"^```[^\n]*\n.*?^```[^\n]*$", "", text, flags=re.M | re.S)
    anchors = set(re.findall(r'<a\s+id=["\']([^"\']+)["\']', text))
    counts: dict[str, int] = {}
    for title in re.findall(r"^#{1,6}\s+(.+?)\s*#*\s*$", text, re.M):
        slug = re.sub(r"[^\w\- ]", "", title.lower()).replace(" ", "-")
        count = counts.get(slug, 0)
        counts[slug] = count + 1
        anchors.add(slug + (f"-{count}" if count else ""))
    return anchors


def read_documentation(
    root: Path,
) -> tuple[dict[str, Path], dict[Path, str], set[Path], list[str]]:
    """Return fixed roles, texts, permitted paths and actionable diagnostics."""
    errors: list[str] = []
    contract = root / "loom-bootstrap/contracts/documentation.toml"
    try:
        config = tomllib.loads(contract.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        return {}, {}, set(), [f"cannot load documentation contract: {error}"]
    if config.get("schema_version") != 1 or config.get("roles") != ROLE_PATHS:
        errors.append("documentation roles must match the declared focused owners")
    roles = {role: Path(path) for role, path in ROLE_PATHS.items()}
    documents = config.get("documents", {})
    archives = documents.get("archives", [])
    exceptions = documents.get("exceptions", [])
    index = documents.get("index")
    if index != "docs/README.md":
        errors.append("documentation index must be docs/README.md")
    names = [*ROLE_PATHS.values(), "docs/README.md"]
    for group, values in (("archives", archives), ("exceptions", exceptions)):
        if not isinstance(values, list) or not all(isinstance(p, str) for p in values):
            errors.append(f"documentation {group} must be an explicit path list")
            continue
        for value in values:
            path = Path(value)
            if (path.is_absolute() or ".." in path.parts
                    or path.suffix != ".md" or ".work" in path.parts):
                errors.append(f"unsafe documentation path: {value}")
            elif group == "archives" and not value.startswith("docs/archive/"):
                errors.append(f"archive outside docs/archive: {value}")
            elif group == "exceptions" and path not in EXCEPTIONS:
                errors.append(f"unapproved documentation exception: {value}")
            else:
                names.append(value)
    if len(names) != len(set(names)):
        errors.append("duplicate documentation path declaration")
    permitted = {Path(name) for name in names}
    texts: dict[Path, str] = {}
    for path in sorted(permitted - EXCEPTIONS):
        try:
            texts[path] = (root / path).read_text(encoding="utf-8")
        except OSError as error:
            errors.append(f"missing declared documentation: {path}: {error}")
    if len(texts.get(roles["entrypoint"], "").encode()) > 5120:
        errors.append("AGENTS.md entrypoint exceeds the 5 KiB routing budget")

    live = set(roles.values()) | {Path("docs/README.md")}
    edges: dict[Path, set[Path]] = {}
    anchors: dict[Path, set[str]] = {}
    for path, text in texts.items():
        if path != roles["current"] and (TRUTH_BEGIN in text or TRUTH_END in text):
            errors.append(f"current-truth marker outside canonical owner: {path}")
        if path not in live:
            continue
        if (path != roles["current"] and re.search(
                r"^\|\s*Order\s*\|\s*Application\s*\|\s*Product status\s*\|", text, re.M)):
            errors.append(f"competing live application status table: {path}")
        edges[path] = set()
        for raw in re.findall(r"\]\(([^)\n]+)\)", text):
            link = urlsplit(raw.strip("<>"))
            if link.scheme or link.netloc:
                continue
            target = (root / path.parent / unquote(link.path)).resolve() if link.path else root / path
            if target.suffix.lower() not in (".md", ".markdown"):
                continue
            if not target.is_relative_to(root.resolve()):
                errors.append(f"documentation link escapes repository: {path}: {raw}")
                continue
            relative = target.relative_to(root.resolve())
            edges[path].add(relative)
            if not target.is_file():
                errors.append(f"broken documentation link: {path}: {raw}")
                continue
            if link.fragment:
                if relative not in anchors:
                    anchors[relative] = markdown_anchors(target.read_text(encoding="utf-8"))
                if unquote(link.fragment) not in anchors[relative]:
                    errors.append(f"broken documentation anchor: {path}: {raw}")
    reachable = {roles["entrypoint"]}
    pending = list(reachable)
    while pending:
        for target in edges.get(pending.pop(), set()) - reachable:
            reachable.add(target)
            pending.append(target)
    for path in sorted(live - reachable):
        errors.append(f"document unreachable from AGENTS.md: {path}")
    return roles, texts, permitted, errors
