#!/usr/bin/env bash
# Gate 11 interoperability check for Loom Present on Linux, using LibreOffice Impress headless.
#
# Usage (inside WSL or on a Linux host, with LibreOffice installed):
#   check_libreoffice.sh <fixture dir>
# The fixture dir holds the files written by
#   LOOM_INTEROP_OUT=<dir> cargo test -p loom-present-core --test interop_fixtures -- --ignored
#
# The fixtures are copied to a private work directory under $HOME (never converted in
# place) and the directory is removed afterwards. Prints PASS/FAIL/INFO/SKIP lines; exit 1
# when a check fails, exit 0 (with a SKIP line) when no soffice is installed.
set -u
fixtures="${1:?usage: check_libreoffice.sh <fixture dir>}"
soffice="$(command -v soffice || command -v libreoffice || true)"
if [ -z "$soffice" ]; then echo "SKIP  LibreOffice (soffice) is not installed"; exit 0; fi

work="$(mktemp -d "$HOME/loom-interop-present.XXXXXX")"
trap 'rm -rf "$work"' EXIT
cp "$fixtures"/present.pptx "$fixtures"/present.pdf "$work"/
cd "$work" || exit 1
profile="-env:UserInstallation=file://$work/profile"
fails=0; passes=0
check() { # name, condition exit code, detail
  if [ "$2" -eq 0 ]; then passes=$((passes + 1)); echo "PASS  $1"; else fails=$((fails + 1)); echo "FAIL  $1  -- $3"; fi
}
echo "INFO  $("$soffice" --version 2>/dev/null | head -1)"

timeout 180 "$soffice" "$profile" --headless --convert-to odp --outdir out present.pptx >/dev/null 2>&1
check "pptx converts to ODP" "$([ -s out/present.odp ]; echo $?)" "no ODP written"
if [ -s out/present.odp ]; then
  content="$(unzip -p out/present.odp content.xml)"
  pages="$(printf '%s' "$content" | grep -o '<draw:page ' | wc -l)"
  check "ODP has 4 slides" "$([ "$pages" -eq 4 ]; echo $?)" "slides: $pages"
  has() { printf '%s' "$content" | grep -qF -- "$1"; echo $?; }
  check "slide text: title, results, labels" "$(has 'Q3 &amp; Q4 Plan')" ""
  check "slide text: Results" "$(has 'Results')" ""
  check "slide text: Box label and 42%" "$(printf '%s' "$content" | grep -qF 'Box label' && printf '%s' "$content" | grep -qF '42%'; echo $?)" ""
  check "subtitle accents and typographic text" "$(has 'Café “review”')" ""
  check "speaker notes survive" "$(has 'Open warmly. Mention the budget.')" ""
  check "picture is a draw:image" "$([ "$(printf '%s' "$content" | grep -o '<draw:image ' | wc -l)" -ge 1 ]; echo $?)" ""
fi
timeout 180 "$soffice" "$profile" --headless --convert-to pdf --outdir out present.pptx >/dev/null 2>&1
check "pptx converts to PDF" "$([ -s out/present.pdf ]; echo $?)" "no PDF written"
if command -v pdftotext >/dev/null 2>&1; then
  pdftotext -layout out/present.pdf out/impress.txt
  check "Impress PDF shows 4 pages" "$([ "$(grep -c $'\f' out/impress.txt)" -ge 3 ]; echo $?)" ""
  pdftotext -layout present.pdf out/loom-present.txt
  check "Loom PDF text: slide titles" "$(grep -qF 'Results' out/loom-present.txt && grep -qF 'Thank you' out/loom-present.txt; echo $?)" ""
  check "Loom PDF text: body lines on separate lines" "$(grep -qF 'First line' out/loom-present.txt && grep -qF 'Second line' out/loom-present.txt && ! grep -F 'First line' out/loom-present.txt | grep -qF 'Second line'; echo $?)" ""
else
  echo "SKIP  pdftotext is not installed; PDF text was not extracted on Linux"
fi
echo "SUMMARY libreoffice present: $passes passed, $fails failed"
[ "$fails" -eq 0 ]
