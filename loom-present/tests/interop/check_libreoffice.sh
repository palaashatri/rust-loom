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
  check "ODP keeps a table" "$(printf '%s' "$content" | grep -q '<table:table'; echo $?)" ""
  check "ODP keeps table cells" "$(printf '%s' "$content" | grep -q 'Header 1'; echo $?)" ""
  check "ODP carries the chart object" "$([ "$(unzip -l out/present.odp | grep -c 'Object [0-9]*/content.xml')" -ge 1 ]; echo $?)" ""
  timeout 180 "$soffice" "$profile" --headless --convert-to pptx --outdir out-rt out/present.odp >/dev/null 2>&1
  check "ODP converts back to PPTX" "$([ -s out-rt/present.pptx ]; echo $?)" "no round-trip PPTX written"
  if [ -s out-rt/present.pptx ]; then
    rt_list="$(unzip -l out-rt/present.pptx)"
    rt_slides="$(printf '%s' "$rt_list" | grep -c 'ppt/slides/slide[0-9]*\.xml')"
    check "round-trip PPTX has 4 slides" "$([ "$rt_slides" -eq 4 ]; echo $?)" "slides: $rt_slides"
    rt_all="$(unzip -p out-rt/present.pptx 'ppt/slides/slide*.xml')"
    check "round-trip PPTX keeps title text" "$(printf '%s' "$rt_all" | grep -qF 'Q3 &amp; Q4 Plan'; echo $?)" ""
    check "round-trip PPTX keeps Results text" "$(printf '%s' "$rt_all" | grep -qF 'Results'; echo $?)" ""
    rt_notes="$(unzip -p out-rt/present.pptx 'ppt/notesSlides/notesSlide*.xml' 2>/dev/null || true)"
    check "round-trip PPTX keeps speaker notes" "$(printf '%s' "$rt_notes" | grep -qF 'Open warmly. Mention the budget.'; echo $?)" ""
    check "round-trip PPTX keeps table" "$(printf '%s' "$rt_all" | grep -q '<a:tbl'; echo $?)" ""
    check "round-trip PPTX keeps picture" "$([ "$(printf '%s' "$rt_list" | grep -c 'ppt/media/')" -ge 1 ]; echo $?)" ""
    check "round-trip PPTX keeps chart" "$([ "$(printf '%s' "$rt_list" | grep -c 'ppt/charts/chart[0-9]*\.xml')" -ge 1 ]; echo $?)" ""

    cli_bin="${LOOM_PRESENT_CLI:-$(command -v loom-present-cli || true)}"
    if [ -n "$cli_bin" ]; then
      "$cli_bin" import-pptx out-rt/present.pptx out-rt/reimported.loomdeck >/dev/null 2>&1
      check "loom-present-cli re-imports round-trip PPTX" "$([ -s out-rt/reimported.loomdeck ]; echo $?)" "import-pptx failed"
      if [ -s out-rt/reimported.loomdeck ]; then
        cli_info="$("$cli_bin" inspect out-rt/reimported.loomdeck 2>/dev/null || true)"
        check "re-imported deck has 4 slides" "$(printf '%s' "$cli_info" | grep -q 'slides: 4'; echo $?)" "$cli_info"
        check "re-imported deck keeps title & notes" "$(printf '%s' "$cli_info" | grep -qF 'Q3 & Q4 Plan' && printf '%s' "$cli_info" | grep -qF 'Open warmly'; echo $?)" ""
      fi
    else
      echo "SKIP  loom-present-cli not found (set LOOM_PRESENT_CLI); skipping semantic re-import"
    fi
  fi
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
