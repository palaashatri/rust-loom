#!/usr/bin/env bash
# Gate 11 interoperability check for Loom Writer on Linux, using LibreOffice Writer headless.
#
# Usage (inside WSL or on a Linux host, with LibreOffice installed):
#   check_libreoffice.sh <fixture dir>
# The fixture dir holds the files written by
#   LOOM_INTEROP_OUT=<dir> cargo test -p loom-writer-core --test interop_fixtures -- --ignored
#
# The fixtures are copied to a private work directory under $HOME (never converted in
# place) and the directory is removed afterwards. Prints PASS/FAIL/INFO/SKIP lines; exit 1
# when a check fails, exit 0 (with a SKIP line) when no soffice is installed.
set -u
fixtures="${1:?usage: check_libreoffice.sh <fixture dir>}"
soffice="$(command -v soffice || command -v libreoffice || true)"
if [ -z "$soffice" ]; then echo "SKIP  LibreOffice (soffice) is not installed"; exit 0; fi

work="$(mktemp -d "$HOME/loom-interop-writer.XXXXXX")"
trap 'rm -rf "$work"' EXIT
cp "$fixtures"/writer.docx "$fixtures"/writer.pdf "$fixtures"/writer.md "$work"/
cd "$work" || exit 1
profile="-env:UserInstallation=file://$work/profile"
fails=0; passes=0
check() { # name, condition exit code, detail
  if [ "$2" -eq 0 ]; then passes=$((passes + 1)); echo "PASS  $1"; else fails=$((fails + 1)); echo "FAIL  $1  -- $3"; fi
}
echo "INFO  $("$soffice" --version 2>/dev/null | head -1)"

timeout 120 "$soffice" "$profile" --headless --convert-to txt:Text --outdir out writer.docx >/dev/null 2>&1
check "docx converts to text" "$([ -s out/writer.txt ]; echo $?)" "no text written"
if [ -s out/writer.txt ]; then
  has() { grep -qF -- "$1" out/writer.txt; echo $?; }
  check "heading text" "$(has 'Quarterly Overview')" ""
  check "paragraph text" "$(has 'Plain bold italic underline mix')" ""
  check "bulleted items" "$(has 'Apples')" ""
  check "numbered items" "$(has 'Third step')" ""
  check "table cells Tea and Coffee" "$(grep -qF Tea out/writer.txt && grep -qF Coffee out/writer.txt; echo $?)" ""
  check "accented and typographic text" "$(has 'Café Zoë – “quoted” — €5 … Düsseldorf')" ""
  check "closing line" "$(has 'Centered closing line')" ""
fi
timeout 120 "$soffice" "$profile" --headless --convert-to pdf --outdir out writer.docx >/dev/null 2>&1
check "docx converts to PDF" "$([ -s out/writer.pdf ]; echo $?)" "no PDF written"
timeout 120 "$soffice" "$profile" --headless --convert-to odt --outdir out writer.docx >/dev/null 2>&1
if [ -s out/writer.odt ]; then
  content="$(unzip -p out/writer.odt content.xml)"
  check "ODT keeps Heading 1 style" "$(printf '%s' "$content" | grep -q 'text:outline-level="1"'; echo $?)" ""
  check "ODT keeps a table" "$(printf '%s' "$content" | grep -q '<table:table '; echo $?)" ""
  check "ODT keeps a list" "$(printf '%s' "$content" | grep -q '<text:list '; echo $?)" ""
  check "ODT keeps the comment (annotation)" "$(printf '%s' "$content" | grep -q '<office:annotation'; echo $?)" ""
  check "ODT keeps bold text" "$(unzip -p out/writer.odt styles.xml content.xml | grep -q 'fo:font-weight="bold"'; echo $?)" ""
else
  check "docx converts to ODT" 1 "no ODT written"
fi
# Loom's own PDF: text extraction through poppler when it is installed.
if command -v pdftotext >/dev/null 2>&1; then
  pdftotext -layout writer.pdf out/loom-writer-pdf.txt
  check "Loom PDF text: heading and list" "$(grep -qF 'Quarterly Overview' out/loom-writer-pdf.txt && grep -qF 'Apples' out/loom-writer-pdf.txt; echo $?)" ""
  check "Loom PDF text: accents" "$(grep -qF 'Café Zoë' out/loom-writer-pdf.txt; echo $?)" ""
else
  echo "SKIP  pdftotext is not installed; Loom's PDF text was not extracted on Linux"
fi
check "Loom Markdown is valid UTF-8" "$(iconv -f UTF-8 -t UTF-8 writer.md >/dev/null 2>&1; echo $?)" ""
echo "SUMMARY libreoffice writer: $passes passed, $fails failed"
[ "$fails" -eq 0 ]
