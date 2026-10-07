<#
  Gate 11 interoperability check for Loom Sheets, using real Microsoft Excel.

  Opens the files written by
    LOOM_INTEROP_OUT=<dir> cargo test -p loom-sheets-core --test interop_fixtures -- --ignored
  (sheets.xlsx, sheets-1.csv, sheets-expected.json) in an invisible Excel
  through COM, read-only, with alerts off, and prints one PASS/FAIL/INFO line
  per check. Exit code 1 when any check FAILs.

  Usage: powershell -NoProfile -ExecutionPolicy Bypass -File check_excel.ps1 -Dir <fixture dir>

  Fixture files are never modified (workbooks open read-only). Only the Excel
  instance started here is quit, and stopped by process id if Quit did not end it.
#>
param([string]$Dir = $env:LOOM_INTEROP_OUT)

$ErrorActionPreference = 'Stop'
if (-not $Dir) { throw 'pass -Dir or set LOOM_INTEROP_OUT' }
$Dir = (Resolve-Path -LiteralPath $Dir).Path

Add-Type -Namespace Loom -Name Win32 -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint processId);
'@
Add-Type -AssemblyName System.IO.Compression.FileSystem

$script:fails = 0
$script:passes = 0
function Check([string]$name, [bool]$ok, [string]$detail = '') {
    if ($ok) { $script:passes++; Write-Output "PASS  $name" }
    else { $script:fails++; Write-Output "FAIL  $name  -- $detail" }
}
function Info([string]$text) { Write-Output "INFO  $text" }
function Try-Get([scriptblock]$Block) { try { & $Block } catch { $null } }
function Rgb([int]$r, [int]$g, [int]$b) { return $r + 256 * $g + 65536 * $b }

function Cached-Values([string]$xlsx, [string]$part) {
    # The <v> value the file itself carries for every cell, keyed by reference.
    $zip = [System.IO.Compression.ZipFile]::OpenRead($xlsx)
    try {
        $entry = $zip.GetEntry($part)
        $reader = New-Object System.IO.StreamReader($entry.Open(), [System.Text.Encoding]::UTF8)
        $xml = $reader.ReadToEnd(); $reader.Close()
    } finally { $zip.Dispose() }
    $map = @{}
    foreach ($m in [regex]::Matches($xml, '<c r="([A-Z]+[0-9]+)"([^>]*)>(?:<f[^>]*>.*?</f>|<f[^>]*/>)?<v>(.*?)</v>')) {
        $map[$m.Groups[1].Value] = @{ attrs = $m.Groups[2].Value; v = $m.Groups[3].Value }
    }
    return $map
}

$xlsx = Join-Path $Dir 'sheets.xlsx'
$expected = Get-Content -LiteralPath (Join-Path $Dir 'sheets-expected.json') -Raw -Encoding UTF8 | ConvertFrom-Json

$excel = New-Object -ComObject Excel.Application
$excelPid = [uint32]0
[void][Loom.Win32]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
$workbook = $null
$csvBook = $null
try {
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $excel.AskToUpdateLinks = $false
    $excel.EnableEvents = $false
    Info "Excel $($excel.Version) build $($excel.Build)"

    $workbook = $excel.Workbooks.Open($xlsx, 0, $true)
    Check 'xlsx opens in Excel without repair' ($workbook.Name -eq 'sheets.xlsx') "workbook name is '$($workbook.Name)'"

    # ---- sheet names
    $names = @(); foreach ($s in $workbook.Worksheets) { $names += $s.Name }
    Check 'sheet names are "Q1 Sales", "Rates"' (($names -join '|') -eq 'Q1 Sales|Rates') ($names -join '|')
    $ws = $workbook.Worksheets.Item('Q1 Sales')
    $rates = $workbook.Worksheets.Item('Rates')

    # ---- cached values in the file vs Loom's values (what the exporter promised)
    $cachedSales = Cached-Values $xlsx 'xl/worksheets/sheet1.xml'
    $badCached = @()
    foreach ($cell in @('D2', 'D3', 'D7', 'B7', 'H3', 'H4', 'H6', 'H7')) {
        $want = $expected.'Q1 Sales'.$cell
        if (-not $cachedSales.ContainsKey($cell)) { $badCached += "$cell missing"; continue }
        $got = $cachedSales[$cell].v
        if ($want.kind -eq 'num') {
            if ([math]::Abs([double]::Parse($got, [Globalization.CultureInfo]::InvariantCulture) - [double]$want.value) -gt 1e-9) { $badCached += "$cell cached $got want $($want.value)" }
        } elseif ($got -ne [string]$want.value) { $badCached += "$cell cached '$got' want '$($want.value)'" }
    }
    Check 'xlsx carries the cached values Loom calculated (formula cells)' ($badCached.Count -eq 0) ($badCached -join '; ')

    # ---- values and formulas after Excel recalculates everything
    $excel.CalculateFull()
    $mismatch = @(); $compared = 0
    foreach ($sheetProp in $expected.PSObject.Properties) {
        $sheet = $workbook.Worksheets.Item($sheetProp.Name)
        foreach ($cellProp in $sheetProp.Value.PSObject.Properties) {
            $want = $cellProp.Value
            $got = $sheet.Range($cellProp.Name).Value2
            $compared++
            switch ($want.kind) {
                'num' {
                    $w = [double]$want.value
                    if (-not ($got -is [double]) -or [math]::Abs($got - $w) -gt 1e-9 * [math]::Max(1, [math]::Abs($w))) { $mismatch += "$($sheetProp.Name)!$($cellProp.Name): Excel '$got' vs Loom $w" }
                }
                'bool' { if ($got -ne [bool]$want.value) { $mismatch += "$($sheetProp.Name)!$($cellProp.Name): Excel '$got' vs Loom $($want.value)" } }
                default { if ([string]$got -ne [string]$want.value) { $mismatch += "$($sheetProp.Name)!$($cellProp.Name): Excel '$got' vs Loom '$($want.value)'" } }
            }
        }
    }
    Check "Excel recalculated values equal Loom's values ($compared cells)" ($mismatch.Count -eq 0) ($mismatch -join '; ')

    $formulas = [ordered]@{
        'D2' = '=B2*C2'; 'D5' = '=B5*C5'; 'B7' = '=SUM(B2:B5)'; 'D7' = '=SUM(D2:D5)'
        'H2' = '=IF(D2>5,"big","small")'; 'H3' = '=VLOOKUP("Pears",A2:C5,3,FALSE)'
        'H4' = '=UPPER(A2)&"-"&LEN(A3)'; 'H5' = '=LEFT(A4,4)&"|"&TRIM("  a  b  ")'
        'H6' = '=Rates!B2*B2'; 'H7' = '=SUM(Rates!B2:B4)'; 'H8' = '=ROUND(D7,1)'
    }
    foreach ($key in $formulas.Keys) {
        $actual = [string]$ws.Range($key).Formula
        Check "formula $key is $($formulas[$key])" ($actual -eq $formulas[$key]) "Excel reports '$actual'"
    }
    Check 'text cells are values, not formulas (A2, A5)' (-not $ws.Range('A2').HasFormula -and -not $ws.Range('A5').HasFormula) ''
    Check 'accented/typographic text survives (A4, A5)' (([string]$ws.Range('A4').Value2 -eq "Caf$([char]0xE9) Zo$([char]0xEB)") -and ([string]$ws.Range('A5').Value2 -eq "D$([char]0xFC)sseldorf $([char]0x2013) $([char]0x201C)quoted$([char]0x201D)")) "A4='$($ws.Range('A4').Value2)'"
    Check 'Rates sheet data (A2=North, B4=0.25)' (([string]$rates.Range('A2').Value2 -eq 'North') -and ([double]$rates.Range('B4').Value2 -eq 0.25)) ''

    # ---- number formats and displayed text
    Check 'C2 currency format $#,##0.00' ($ws.Range('C2').NumberFormat -eq '$#,##0.00') "format '$($ws.Range('C2').NumberFormat)'"
    Check 'C2 displays $1.25' ($ws.Range('C2').Text -eq '$1.25') "text '$($ws.Range('C2').Text)'"
    Check 'E2 date format yyyy-mm-dd' ($ws.Range('E2').NumberFormat -eq 'yyyy-mm-dd') "format '$($ws.Range('E2').NumberFormat)'"
    Check 'E2 displays 2024-03-15' ($ws.Range('E2').Text -eq '2024-03-15') "text '$($ws.Range('E2').Text)'"
    Check 'F2 percent format 0.0%' ($ws.Range('F2').NumberFormat -eq '0.0%') "format '$($ws.Range('F2').NumberFormat)'"
    Check 'F2 displays 25.6%' ($ws.Range('F2').Text -eq '25.6%') "text '$($ws.Range('F2').Text)'"
    Check 'D7 number format #,##0.00' ($ws.Range('D7').NumberFormat -eq '#,##0.00') "format '$($ws.Range('D7').NumberFormat)'"

    # ---- cell styles
    $boldHeaders = $true; foreach ($c in 'A1', 'B1', 'C1', 'D1', 'E1', 'F1') { if (-not $ws.Range($c).Font.Bold) { $boldHeaders = $false } }
    Check 'header cells A1:F1 are bold' $boldHeaders ''
    Check 'A2 is not bold' (-not $ws.Range('A2').Font.Bold) ''
    Check 'header fill is gray E5E7EB' ($ws.Range('A1').Interior.Color -eq (Rgb 0xE5 0xE7 0xEB)) "color $($ws.Range('A1').Interior.Color)"
    Check 'D7 fill is yellow FEF08A, bold, single underline' (($ws.Range('D7').Interior.Color -eq (Rgb 0xFE 0xF0 0x8A)) -and $ws.Range('D7').Font.Bold -and ($ws.Range('D7').Font.Underline -eq 2)) "color $($ws.Range('D7').Interior.Color) bold $($ws.Range('D7').Font.Bold) underline $($ws.Range('D7').Font.Underline)"
    Check 'F2 is italic' ([bool]$ws.Range('F2').Font.Italic) ''
    $bordered = $true
    foreach ($edge in 7, 8, 9, 10) { if ($ws.Range('B1').Borders.Item($edge).LineStyle -eq -4142) { $bordered = $false } }
    Check 'header cell B1 has all four borders' $bordered ''
    Check 'D2 has no border' ($ws.Range('D2').Borders.Item(7).LineStyle -eq -4142) ''
    Check 'A7 is right aligned' ($ws.Range('A7').HorizontalAlignment -eq -4152) "alignment $($ws.Range('A7').HorizontalAlignment)"

    # ---- sizes (Loom pixels * 0.75 = Excel points; Excel quantizes to character widths)
    $wA = [double]$ws.Columns.Item(1).Width; $wB = [double]$ws.Columns.Item(2).Width; $wD = [double]$ws.Columns.Item(4).Width
    Check 'column A width about 150 px (112.5 pt)' ([math]::Abs($wA - 112.5) -le 5) "width $wA pt"
    Check 'column D width about 90 px (67.5 pt)' ([math]::Abs($wD - 67.5) -le 5) "width $wD pt"
    Check 'column B keeps the default width (about 80 px = 60 pt)' ([math]::Abs($wB - 60) -le 5) "width $wB pt"
    Check 'row 1 height about 30 px (22.5 pt)' ([math]::Abs([double]$ws.Rows.Item(1).RowHeight - 22.5) -le 1.5) "height $($ws.Rows.Item(1).RowHeight) pt"

    # ---- freeze panes (an invisible Excel catches up late, so retry)
    $ws.Activate()
    $frozen = $false; $splitRow = 0; $splitCol = 0
    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        $active = Try-Get { $excel.ActiveWindow }
        $frozen = [bool](Try-Get { $active.FreezePanes })
        $splitRow = [int](Try-Get { $active.SplitRow }); $splitCol = [int](Try-Get { $active.SplitColumn })
        if ($frozen) { break }
        Start-Sleep -Milliseconds 200
    }
    Check 'freeze panes: 1 row and 1 column' ($frozen -and $splitRow -eq 1 -and $splitCol -eq 1) "frozen=$frozen row=$splitRow col=$splitCol"

    # ---- chart and shape
    $charts = $ws.ChartObjects()
    Check 'one chart on Q1 Sales' ($charts.Count -eq 1) "count $($charts.Count)"
    if ($charts.Count -ge 1) {
        $chart = $charts.Item(1).Chart
        Check 'chart is a clustered column chart (51)' ($chart.ChartType -eq 51) "type $($chart.ChartType)"
        Check 'chart title is "Totals by item"' ($chart.HasTitle -and $chart.ChartTitle.Text -eq 'Totals by item') "title '$(Try-Get { $chart.ChartTitle.Text })'"
        $series = $chart.SeriesCollection(1)
        $formula = [string]$series.Formula
        Check 'chart series reads Q1 Sales D2:D5 with A2:A5 categories' (($formula -like "*'Q1 Sales'!`$D`$2:`$D`$5*") -and ($formula -like "*'Q1 Sales'!`$A`$2:`$A`$5*")) "series formula $formula"
        $vals = @($series.Values)
        Check 'chart plots the calculated totals (3.75, 8, 31.5, 24)' (($vals.Count -eq 4) -and ([math]::Abs($vals[0] - 3.75) -lt 1e-9) -and ([math]::Abs($vals[2] - 31.5) -lt 1e-9)) "values $($vals -join ',')"
    }
    $shapeText = @(); foreach ($sh in $ws.Shapes) { $t = Try-Get { $sh.TextFrame2.TextRange.Text }; if ($t) { $shapeText += $t } }
    Check 'shape "Interop shape" is present' ($shapeText -contains 'Interop shape') "shape texts: $($shapeText -join ',')"

    $workbook.Close($false); $workbook = $null

    # ---- CSV: Excel's plain open (what a user double-clicking does) and an explicit UTF-8 import
    $csv = Join-Path $Dir 'sheets-1.csv'
    $bytes = [System.IO.File]::ReadAllBytes($csv)
    $hasBom = ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF)
    $csvBook = $excel.Workbooks.Open($csv, 0, $true)
    $plain = [string]$csvBook.Worksheets.Item(1).Range('A4').Value2
    $csvBook.Close($false); $csvBook = $null
    $wantCafe = "Caf$([char]0xE9) Zo$([char]0xEB)"
    if ($plain -eq $wantCafe) { Check 'CSV opened plainly in Excel keeps accents' $true }
    else { Check 'CSV opened plainly in Excel keeps accents' $false "Excel shows '$plain' (file has BOM: $hasBom); Excel assumes the ANSI code page without a UTF-8 BOM" }
    # Workbooks.OpenText with Origin 65001 (UTF-8), comma-delimited, double-quote qualifier.
    # Excel ignores Origin for a file named *.csv, so import a temporary .txt copy.
    $txtCopy = Join-Path ([IO.Path]::GetTempPath()) ('loom-interop-' + [Guid]::NewGuid().ToString('N') + '.txt')
    Copy-Item -LiteralPath $csv -Destination $txtCopy
    try {
        $excel.Workbooks.OpenText($txtCopy, 65001, 1, 1, 1, $false, $false, $false, $true, $false, $false)
        $csvBook = $excel.ActiveWorkbook
    } finally { Remove-Item -LiteralPath $txtCopy -ErrorAction SilentlyContinue }
    $sheet1 = $csvBook.Worksheets.Item(1)
    Check 'CSV imported as UTF-8 keeps accents and typographic text' (([string]$sheet1.Range('A4').Value2 -eq $wantCafe) -and ([string]$sheet1.Range('A5').Value2 -eq "D$([char]0xFC)sseldorf $([char]0x2013) $([char]0x201C)quoted$([char]0x201D)")) "A4='$($sheet1.Range('A4').Value2)'"
    Check 'CSV holds calculated values (D2=3.75, H3=0.8, D7=67.25)' (([double]$sheet1.Range('D2').Value2 -eq 3.75) -and ([double]$sheet1.Range('H3').Value2 -eq 0.8) -and ([double]$sheet1.Range('D7').Value2 -eq 67.25)) ''
    Check 'CSV has no formulas (values only)' (-not $sheet1.Range('D2').HasFormula) ''
    $csvBook.Close($false); $csvBook = $null
}
catch {
    $script:fails++
    Write-Output "FAIL  unexpected error  -- $($_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
}
finally {
    foreach ($b in $csvBook, $workbook) { if ($b) { try { $b.Close($false) } catch {} } }
    try { $excel.Quit() } catch {}
    foreach ($o in $csvBook, $workbook, $ws, $rates, $excel) { if ($o) { try { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($o) } catch {} } }
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    Start-Sleep -Milliseconds 500
    if ($excelPid -ne 0 -and (Get-Process -Id $excelPid -ErrorAction SilentlyContinue)) { Stop-Process -Id $excelPid -Force -ErrorAction SilentlyContinue }
}
Write-Output "SUMMARY excel: $script:passes passed, $script:fails failed"
if ($script:fails -gt 0) { exit 1 }
