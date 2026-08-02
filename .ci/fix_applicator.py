from pathlib import Path

path = Path(__file__).resolve().parent / "apply_native_productisation.py"
text = path.read_text(encoding="utf-8")

replacements = (
    (
        "    helpers = '''\n\ndef linux_mime_xml() -> str:",
        '    helpers = r"""\n\ndef linux_mime_xml() -> str:',
    ),
    (
        "\n'''\n    if marker not in text:\n        raise RuntimeError(\"packaging helper insertion point not found\")",
        '\n"""\n    if marker not in text:\n        raise RuntimeError("packaging helper insertion point not found")',
    ),
    (
        "    wix_function = '''def wix_source(binaries: dict[str, Path], version: str, architecture: str) -> str:",
        '    wix_function = r"""def wix_source(binaries: dict[str, Path], version: str, architecture: str) -> str:',
    ),
    (
        "\n\ndef xml_escape'''\n    text = text[: wix_match.start()] + wix_function + text[wix_match.end() :]",
        '\n\ndef xml_escape"""\n    text = text[: wix_match.start()] + wix_function + text[wix_match.end() :]',
    ),
)

for old, new in replacements:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"expected one applicator delimiter block, found {count}: {old[:40]!r}")
    text = text.replace(old, new, 1)

old_field_patch = '''    text = replace_once(
        text,
        '        text: root.value;\\n        edited => {',
        '        text: root.value;\\n        enabled: root.enabled;\\n        edited => {',
        "Field enabled binding",
    )'''
new_field_patch = '''    text = replace_once(
        text,
        '        color: Theme.palette().ink;\\n        text: root.value;\\n        edited => {',
        '        color: Theme.palette().ink;\\n        text: root.value;\\n        enabled: root.enabled;\\n        edited => {',
        "Field enabled binding",
    )'''
count = text.count(old_field_patch)
if count != 1:
    raise RuntimeError(f"expected one Field patch definition, found {count}")
text = text.replace(old_field_patch, new_field_patch, 1)

path.write_text(text, encoding="utf-8", newline="\n")
