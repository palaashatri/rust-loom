#!/usr/bin/env bash
# Gate 11 interoperability check for Loom Sheets on Linux, using LibreOffice Calc headless.
#
# Usage (inside WSL or on a Linux host, with LibreOffice installed):
#   check_libreoffice.sh <fixture dir>
# The fixture dir holds the files written by
#   LOOM_INTEROP_OUT=<dir> cargo test -p loom-sheets-core --test interop_fixtures -- --ignored
#
# The fixtures are copied to a private work directory under $HOME (never converted in
# place) and the directory is removed afterwards. Prints PASS/FAIL/INFO/SKIP lines; exit 1
# when a check fails, exit 0 (with a SKIP line) when no soffice is installed.
set -u
fixtures="${1:?usage: check_libreoffice.sh <fixture dir>}"
soffice="$(command -v soffice || command -v libreoffice || true)"
if [ -z "$soffice" ]; then echo "SKIP  LibreOffice (soffice) is not installed"; exit 0; fi

work="$(mktemp -d "$HOME/loom-interop-sheets.XXXXXX")"
trap 'rm -rf "$work"' EXIT
cp "$fixtures"/sheets.xlsx "$fixtures"/sheets-1.csv "$work"/
cd "$work" || exit 1
profile="-env:UserInstallation=file://$work/profile"
fails=0; passes=0
check() { # name, condition exit code, detail
  if [ "$2" -eq 0 ]; then passes=$((passes + 1)); echo "PASS  $1"; else fails=$((fails + 1)); echo "FAIL  $1  -- $3"; fi
}
echo "INFO  $("$soffice" --version 2>/dev/null | head -1)"

timeout 120 "$soffice" "$profile" --headless --convert-to pdf --outdir out sheets.xlsx >/dev/null 2>&1
check "xlsx converts to PDF" "$([ -s out/sheets.pdf ]; echo $?)" "no PDF written"
# First sheet to CSV through Calc's own recalculation (filter options: comma, quote, UTF-8).
timeout 120 "$soffice" "$profile" --headless --convert-to 'csv:Text - txt - csv (StarCalc):44,34,76,1' --outdir out sheets.xlsx >/dev/null 2>&1
csv="$(ls out/sheets*.csv 2>/dev/null | head -1)"
check "xlsx converts to CSV" "$([ -n "$csv" ]; echo $?)" "no CSV written"
if [ -n "$csv" ]; then
  has() { grep -qF -- "$1" "$csv"; echo $?; }
  check "header row Item,Qty,Price,Total" "$(has '"Item","Qty","Price","Total"')" "$(head -1 "$csv")"
  check "accented text Café Zoë survives" "$(has 'Café Zoë')" ""
  check "typographic text survives" "$(has 'Düsseldorf – “quoted”')" ""
  check "formula D2=B2*C2 calculates 3.75" "$(has '3.75')" ""
  check "IF formula gives small" "$(has 'small')" ""
  check "text formula gives APPLES-5" "$(has 'APPLES-5')" ""
  check "cross-sheet formula gives 4.5" "$(has ',4.5')" ""
  check "SUM total 67.25 (D7)" "$(has '67.25')" ""
  check "currency format shows \$1.25" "$(has '$1.25')" "$(sed -n 2p "$csv")"
  check "date format shows 2024-03-15" "$(has '2024-03-15')" "$(sed -n 2p "$csv")"
  check "percent format shows 25.6%" "$(has '25.6%')" "$(sed -n 2p "$csv")"
fi
# Round trip: Calc rewrites the workbook as ODS, so a package it cannot parse fails here.
timeout 120 "$soffice" "$profile" --headless --convert-to ods --outdir out sheets.xlsx >/dev/null 2>&1
check "xlsx converts to ODS" "$([ -s out/sheets.ods ]; echo $?)" "no ODS written"
if [ -s out/sheets.ods ]; then
  sheets="$(unzip -p out/sheets.ods content.xml | grep -o '<table:table table:name="[^"]*"' | sed 's/.*name="//; s/"$//' | paste -sd'|')"
  check "ODS has sheets Q1 Sales|Rates" "$([ "$sheets" = 'Q1 Sales|Rates' ]; echo $?)" "sheets: $sheets"
  charts="$(unzip -l out/sheets.ods | grep -c 'Object [0-9]*/content.xml')"
  check "ODS carries the chart object" "$([ "$charts" -ge 1 ]; echo $?)" "chart objects: $charts"
fi
# The CSV Loom wrote itself must read back as UTF-8 text.
check "Loom CSV is valid UTF-8" "$(iconv -f UTF-8 -t UTF-8 sheets-1.csv >/dev/null 2>&1; echo $?)" ""
echo "SUMMARY libreoffice sheets: $passes passed, $fails failed"
[ "$fails" -eq 0 ]
