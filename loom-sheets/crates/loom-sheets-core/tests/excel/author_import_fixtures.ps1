<#
  Authors the small XLSX fixtures used by the Loom import regression tests,
  with real Microsoft Excel driven invisibly through COM.

  Usage: powershell -NoProfile -ExecutionPolicy Bypass -File author_import_fixtures.ps1 -OutDir <dir>
  Writes <dir>/excel_basic, excel_features, excel_visuals, excel_edge and excel_1904 (.xlsx).
  Only the Excel instance started here is stopped, by process id.
#>
param([Parameter(Mandatory = $true)][string]$OutDir)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path

Add-Type -Namespace Loom -Name Win32 -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint processId);
'@
$xl = New-Object -ComObject Excel.Application
$xl.Visible = $false
$xl.DisplayAlerts = $false
[uint32]$excelPid = 0
[void][Loom.Win32]::GetWindowThreadProcessId([System.IntPtr]$xl.Hwnd, [ref]$excelPid)
try {
    # ---- 1. basic: values, formats, formulas, text, dates, booleans, errors
    $wb = $xl.Workbooks.Add()
    $ws = $wb.Worksheets.Item(1)
    $ws.Name = 'Data & Q1'
    $ws.Range('A1').Value2 = 'Item'
    $ws.Range('B1').Value2 = 'Amount'
    $ws.Range('C1').Value2 = 'Done'
    $ws.Range('A2').Value2 = 'Rent'
    $ws.Range('A3').Value2 = 'Food'
    $ws.Range('A4').Value2 = 'Caf' + [char]0xE9 + ' ' + [char]0x65E5 + [char]0x672C
    $ws.Range('B2').Value2 = 1200.5
    $ws.Range('B3').Value2 = 450
    $ws.Range('B4').Value2 = 0.1
    $ws.Range('C2').Value2 = $true
    $ws.Range('C3').Value2 = $false
    $ws.Range('B2:B4').NumberFormat = '$#,##0.00'
    $ws.Range('B5').Formula = '=SUM(B2:B4)'
    $ws.Range('B6').Formula = '=AVERAGE(B2:B4)'
    $ws.Range('A5').Value2 = 'Total'
    $ws.Range('A6').Value2 = 'Mean'
    $ws.Range('D1').Value2 = 'Doubled'
    $ws.Range('D2').Formula = '=B2*2'
    $ws.Range('D2:D4').FillDown() | Out-Null
    $ws.Range('E1').Formula = '=1/0'
    $ws.Range('E2').Formula = '=NA()'
    $ws.Range('E3').Formula = '=IF(C2,"yes","no")'
    $ws.Range('F1').Value2 = [double]45292
    $ws.Range('F1').NumberFormat = 'yyyy-mm-dd'
    $ws.Range('F2').Value2 = 0.256
    $ws.Range('F2').NumberFormat = '0.0%'
    $ws.Range('F3').Value2 = "line one`nline two"
    $ws.Range('F4').Value2 = 1234567.891
    $ws.Range('F4').NumberFormat = '0.00E+00'
    $ws.Range('G1').NumberFormat = '@'
    $ws.Range('G1').Value2 = '007'
    $ws.Range('G2').Formula = '=TEXT(F1,"dd/mm/yyyy")'
    $ws.Range('G3').Formula = '=A2&" & "&A3'
    $ws.Range('G4').Formula = '=SUM(B2:B4)/COUNT(B2:B4)'
    $ws.Range('H1').Value2 = 'plain bold'
    $ws.Range('H1').Characters(7, 4).Font.Bold = $true
    $ws2 = $wb.Worksheets.Add([Type]::Missing, $ws)
    $ws2.Name = 'Data'
    $ws2.Range('A1').Value2 = 'Other'
    $ws2.Range('B1').Formula = "='Data & Q1'!B2*2"
    $ws2.Range('B2').Formula = '=Data!A1'
    $ws2.Range('A3').Value2 = 'in hidden row'
    $ws2.Range('C1').Value2 = 'in hidden col'
    $ws2.Rows.Item(3).Hidden = $true
    $ws2.Columns.Item(3).Hidden = $true
    $ws3 = $wb.Worksheets.Add([Type]::Missing, $ws2)
    $ws3.Name = 'Secret'
    $ws3.Range('A1').Value2 = 'hidden sheet'
    $ws3.Visible = 0
    $wb.SaveAs((Join-Path $OutDir 'excel_basic.xlsx'), 51)
    $wb.Close($false)

    # ---- 2. features: styles, merge, widths, freeze, names, cf, validation, array
    $wb = $xl.Workbooks.Add()
    $ws = $wb.Worksheets.Item(1)
    $ws.Name = 'Styled'
    $ws.Range('A1').Value2 = 'Header'
    $ws.Range('A1').Font.Bold = $true
    $ws.Range('A1').Font.Italic = $true
    $ws.Range('A1').Font.Underline = 2
    $ws.Range('A1').Font.Size = 16
    $ws.Range('A1').Interior.Color = 0x00FFFF
    $ws.Range('A1').HorizontalAlignment = -4108
    $ws.Range('A2').Value2 = 'Right'
    $ws.Range('A2').HorizontalAlignment = -4152
    $ws.Range('A3').Value2 = 'Boxed'
    $ws.Range('A3').Borders.LineStyle = 1
    $ws.Range('B1').Value2 = 5
    $ws.Range('B1').NumberFormat = '0.000'
    $ws.Range('B2').Value2 = 5
    $ws.Range('B2').NumberFormat = '#,##0'
    $ws.Range('B3').Value2 = 0.5
    $ws.Range('B3').NumberFormat = '0%'
    $ws.Range('B4').Value2 = 5
    $ws.Range('B4').NumberFormat = '[$EUR] #,##0.00'
    $ws.Range('A5:C5').Merge()
    $ws.Range('A5').Value2 = 'Merged banner'
    $ws.Columns.Item(1).ColumnWidth = 30
    $ws.Rows.Item(2).RowHeight = 40
    $ws.Activate()
    $xl.ActiveWindow.FreezePanes = $false
    $ws.Range('B2').Select() | Out-Null
    $xl.ActiveWindow.FreezePanes = $true
    $wb.Names.Add('Rate', '=Styled!$B$1') | Out-Null
    $ws.Range('D1').Value2 = 10
    $ws.Range('D2').Value2 = 20
    $ws.Range('D3').Value2 = 30
    $fc = $ws.Range('D1:D3').FormatConditions.Add(1, 5, '=15')
    $fc.Interior.Color = 0x0000FF
    $ws.Range('E1').Validation.Add(3, 1, 1, 'a,b,c') | Out-Null
    $ws.Range('F1:F2').FormulaArray = '=D1:D2*2'
    $ws.Range('G1').Formula = '=Rate*2'
    $ws.Range('H1').AddComment('a note') | Out-Null
    $ws.Hyperlinks.Add($ws.Range('H2'), 'https://example.com') | Out-Null
    $ws.Range('J1').Value2 = 'Key'
    $ws.Range('K1').Value2 = 'Val'
    $ws.Range('J2').Value2 = 'a'
    $ws.Range('K2').Value2 = 1
    $ws.ListObjects.Add(1, $ws.Range('J1:K2'), $null, 1) | Out-Null
    $wb.SaveAs((Join-Path $OutDir 'excel_features.xlsx'), 51)
    $wb.Close($false)

    # ---- 3. visuals: chart + picture + shape
    $wb = $xl.Workbooks.Add()
    $ws = $wb.Worksheets.Item(1)
    $ws.Name = 'Visuals'
    $ws.Range('A1').Value2 = 'Item'
    $ws.Range('B1').Value2 = 'Amount'
    $ws.Range('A2').Value2 = 'Rent'
    $ws.Range('B2').Value2 = 1200
    $ws.Range('A3').Value2 = 'Food'
    $ws.Range('B3').Value2 = 450
    $ws.Range('A4').Value2 = 'Fun'
    $ws.Range('B4').Value2 = 150
    $co = $ws.ChartObjects().Add(200, 20, 300, 180)
    $co.Chart.ChartType = 51
    $co.Chart.SetSourceData($ws.Range('A1:B4'))
    $co.Chart.HasTitle = $true
    $co.Chart.ChartTitle.Text = 'Spend'
    $png = Join-Path $OutDir 'pixel.png'
    [IO.File]::WriteAllBytes($png, [Convert]::FromBase64String('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='))
    $ws.Shapes.AddPicture($png, 0, -1, 20, 220, 40, 40) | Out-Null
    $sh = $ws.Shapes.AddShape(1, 300, 220, 120, 40)
    $sh.TextFrame2.TextRange.Text = 'Note box'
    $wb.SaveAs((Join-Path $OutDir 'excel_visuals.xlsx'), 51)
    $wb.Close($false)
    Remove-Item $png -ErrorAction SilentlyContinue

    # ---- 4. edge: quoting, newer functions, dynamic arrays, time, 1904 dates
    $wb = $xl.Workbooks.Add()
    $ws = $wb.Worksheets.Item(1)
    $ws.Name = "It's 100%"
    $ws.Range('A1').Value2 = 'x'
    $ws.Range('A2').Value2 = 'y'
    $ws.Range('B1').Formula2 = '=CONCAT(A1,A2)'
    $ws.Range('B2').Formula2 = '=TEXTJOIN("-",TRUE,A1:A2)'
    $ws.Range('B3').Formula = '=IFERROR(1/0,"none")'
    $ws.Range('B4').Formula = '=STDEV.S(C1:C3)'
    $ws.Range('C1').Value2 = 2
    $ws.Range('C2').Value2 = 4
    $ws.Range('C3').Value2 = 9
    $ws.Range('D1').Formula2 = '=SEQUENCE(2,2)'
    $ws.Range('F1').Value2 = 0.5
    $ws.Range('F1').NumberFormat = 'h:mm AM/PM'
    $ws.Range('F2').Value2 = 45292.75
    $ws.Range('F2').NumberFormat = 'yyyy-mm-dd hh:mm'
    $ws.Range('F3').NumberFormat = '@'
    $ws.Range('F3').Value2 = '=not a formula'
    $ws2 = $wb.Worksheets.Add([Type]::Missing, $ws)
    $ws2.Name = 'Refs'
    $ws2.Range('A1').Formula = "='It''s 100%'!C3+1"
    $wb.SaveAs((Join-Path $OutDir 'excel_edge.xlsx'), 51)
    $wb.Close($false)

    $wb = $xl.Workbooks.Add()
    $wb.Date1904 = $true
    $ws = $wb.Worksheets.Item(1)
    $ws.Range('A1').Value2 = 100
    $ws.Range('A1').NumberFormat = 'yyyy-mm-dd'
    $wb.SaveAs((Join-Path $OutDir 'excel_1904.xlsx'), 51)
    $wb.Close($false)
}
finally {
    try { $xl.Quit() } catch {}
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($xl)
    if ($excelPid -ne 0) { Stop-Process -Id $excelPid -Force -ErrorAction SilentlyContinue }
}
