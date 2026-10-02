<#
  Opens an .xlsx in real Microsoft Excel (invisible, read-only, alerts off)
  through COM and writes everything Excel reports about it as JSON, so the
  Loom export tests can compare Excel's view with the Loom model.

  Usage: powershell -NoProfile -ExecutionPolicy Bypass -File dump_excel.ps1 -Path <xlsx> -Out <json>

  Only the Excel instance started here is stopped, by process id, so a
  workbook another tool has open is never disturbed.
#>
param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Out
)

$ErrorActionPreference = 'Stop'

Add-Type -Namespace Loom -Name Win32 -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint processId);
'@

function Try-Get([scriptblock]$Block) {
    try { & $Block } catch { $null }
}

function Value-Kind($v) {
    if ($null -eq $v) { return 'empty' }
    if ($v -is [string]) { return 'string' }
    if ($v -is [bool]) { return 'bool' }
    if ($v -is [int]) { return 'error' }
    return 'number'
}

function Dump-Cell($cell) {
    $v = Try-Get { $cell.Value2 }
    $kind = Value-Kind $v
    $border = {
        param($index)
        $style = Try-Get { $cell.Borders.Item($index).LineStyle }
        if ($null -eq $style) { return 0 }
        return [int]$style
    }
    [ordered]@{
        ref          = $cell.Address($false, $false)
        formula      = [string](Try-Get { $cell.Formula })
        formula2     = [string](Try-Get { $cell.Formula2 })
        hasFormula   = [bool](Try-Get { $cell.HasFormula })
        hasArray     = [bool](Try-Get { $cell.HasArray })
        hasSpill     = [bool](Try-Get { $cell.HasSpill })
        spillRange   = [string](Try-Get { $cell.SpillingToRange.Address($false, $false) })
        kind         = $kind
        value        = if ($kind -eq 'error') { [string](Try-Get { $cell.Text }) } else { $v }
        text         = [string](Try-Get { $cell.Text })
        numberFormat = [string](Try-Get { $cell.NumberFormat })
        bold         = [bool](Try-Get { $cell.Font.Bold })
        italic       = [bool](Try-Get { $cell.Font.Italic })
        underline    = [int](Try-Get { $cell.Font.Underline })
        fontSize     = [double](Try-Get { $cell.Font.Size })
        fontName     = [string](Try-Get { $cell.Font.Name })
        fillPattern  = [int](Try-Get { $cell.Interior.Pattern })
        fillColor    = [int](Try-Get { $cell.Interior.Color })
        borderLeft   = (& $border 7)
        borderTop    = (& $border 8)
        borderBottom = (& $border 9)
        borderRight  = (& $border 10)
        hAlign       = [int](Try-Get { $cell.HorizontalAlignment })
        wrap         = [bool](Try-Get { $cell.WrapText })
    }
}

$excel = New-Object -ComObject Excel.Application
$excelPid = [uint32]0
[void][Loom.Win32]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
$workbook = $null
try {
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $excel.AskToUpdateLinks = $false
    $excel.EnableEvents = $false
    $full = (Resolve-Path -LiteralPath $Path).Path
    # UpdateLinks = 0, ReadOnly = $true; a package Excel has to repair opens
    # with its name changed or fails here.
    $workbook = $excel.Workbooks.Open($full, 0, $true)
    $window = Try-Get { $workbook.Windows.Item(1) }
    $report = [ordered]@{
        excelVersion = [string]$excel.Version
        excelBuild   = [string]$excel.Build
        workbookName = [string]$workbook.Name
        caption      = [string](Try-Get { $window.Caption })
        sheets       = @()
    }
    $loaded = @{}
    foreach ($ws in $workbook.Worksheets) {
        $used = $ws.UsedRange
        $firstRow = $used.Row
        $firstCol = $used.Column
        $rows = $used.Rows.Count
        $cols = $used.Columns.Count
        $cells = @()
        for ($r = $firstRow; $r -lt $firstRow + $rows; $r++) {
            for ($c = $firstCol; $c -lt $firstCol + $cols; $c++) {
                $cells += , (Dump-Cell $ws.Cells.Item($r, $c))
            }
        }
        $ws.Activate()
        # The window of an invisible Excel catches up with the activated sheet
        # a moment late, so read the freeze state until it settles.
        $freeze = $null
        for ($attempt = 0; $attempt -lt 8; $attempt++) {
            $active = Try-Get { $excel.ActiveWindow }
            $freeze = [ordered]@{
                frozen      = [bool](Try-Get { $active.FreezePanes })
                splitRow    = [int](Try-Get { $active.SplitRow })
                splitColumn = [int](Try-Get { $active.SplitColumn })
            }
            if ($freeze.frozen) { break }
            Start-Sleep -Milliseconds 150
        }
        $columnSizes = @()
        for ($c = 1; $c -le [Math]::Min(30, $firstCol + $cols + 2); $c++) {
            $columnSizes += , ([ordered]@{ index = $c; width = [double]$ws.Columns.Item($c).Width; chars = [double]$ws.Columns.Item($c).ColumnWidth })
        }
        $rowSizes = @()
        for ($r = 1; $r -le [Math]::Min(40, $firstRow + $rows + 2); $r++) {
            $rowSizes += , ([ordered]@{ index = $r; height = [double]$ws.Rows.Item($r).RowHeight })
        }
        $charts = @()
        foreach ($co in $ws.ChartObjects()) {
            $chart = $co.Chart
            $series = @()
            foreach ($s in $chart.SeriesCollection()) {
                $series += , ([ordered]@{
                    name    = [string](Try-Get { $s.Name })
                    formula = [string](Try-Get { $s.Formula })
                    type    = [int](Try-Get { $s.ChartType })
                })
            }
            $charts += , ([ordered]@{
                name     = [string]$co.Name
                chartType = [int](Try-Get { $chart.ChartType })
                hasTitle = [bool](Try-Get { $chart.HasTitle })
                title    = [string](Try-Get { $chart.ChartTitle.Text })
                topLeft  = [string](Try-Get { $co.TopLeftCell.Address($false, $false) })
                width    = [double]$co.Width
                height   = [double]$co.Height
                series   = $series
            })
        }
        $shapes = @()
        foreach ($sh in $ws.Shapes) {
            $text = Try-Get { $sh.TextFrame2.TextRange.Text }
            $shapes += , ([ordered]@{
                name        = [string]$sh.Name
                type        = [int]$sh.Type
                autoShape   = [int](Try-Get { $sh.AutoShapeType })
                topLeft     = [string](Try-Get { $sh.TopLeftCell.Address($false, $false) })
                width       = [double]$sh.Width
                height      = [double]$sh.Height
                text        = [string]$text
                fillVisible = [bool](Try-Get { $sh.Fill.Visible })
                fillRgb     = [int](Try-Get { $sh.Fill.ForeColor.RGB })
                altText     = [string](Try-Get { $sh.AlternativeText })
            })
        }
        $report.sheets += , ([ordered]@{
            name           = [string]$ws.Name
            usedRange      = [string]$used.Address($false, $false)
            standardWidth  = [double]$ws.StandardWidth
            standardHeight = [double]$ws.StandardHeight
            freeze         = $freeze
            columns        = $columnSizes
            rows           = $rowSizes
            cells          = $cells
            charts         = $charts
            shapes         = $shapes
            pictures       = [int](Try-Get { $ws.Pictures().Count })
        })
    }
    # Recalculate everything and report what Excel computes for each formula
    # cell, so engine differences show up next to the cached values.
    $excel.CalculateFull()
    $recalculated = @()
    foreach ($ws in $workbook.Worksheets) {
        foreach ($cell in $ws.UsedRange.Cells) {
            if (Try-Get { $cell.HasFormula }) {
                $v = Try-Get { $cell.Value2 }
                $kind = Value-Kind $v
                $recalculated += , ([ordered]@{
                    sheet = [string]$ws.Name
                    ref   = $cell.Address($false, $false)
                    kind  = $kind
                    value = if ($kind -eq 'error') { [string](Try-Get { $cell.Text }) } else { $v }
                })
            }
        }
    }
    $report.recalculated = $recalculated
    $json = $report | ConvertTo-Json -Depth 8
    [System.IO.File]::WriteAllText($Out, $json, (New-Object System.Text.UTF8Encoding($false)))
}
finally {
    if ($workbook) { Try-Get { $workbook.Close($false) } | Out-Null }
    Try-Get { $excel.Quit() } | Out-Null
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
    Start-Sleep -Milliseconds 500
    if ($excelPid -ne 0) {
        $left = Get-Process -Id $excelPid -ErrorAction SilentlyContinue
        if ($left) { Stop-Process -Id $excelPid -Force -ErrorAction SilentlyContinue }
    }
}
