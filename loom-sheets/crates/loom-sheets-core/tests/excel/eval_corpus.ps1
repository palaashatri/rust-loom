<#
  Regenerates the expected values in tests/fixtures/excel_corpus.json by
  evaluating every formula of that fixture in real Microsoft Excel (COM).

  Each formula is entered at AD<1 + 10 * n> of a blank workbook that holds the
  fixture's shared data block, and its Value2 is recorded: numbers, text,
  booleans, error literals (Excel's Int32 error codes are mapped back to
  #DIV/0! and friends) or "syntax" when Excel refuses the formula on entry.

  To add formulas, append [formula, "num", 0] rows to "cases", then run:
    powershell -NoProfile -ExecutionPolicy Bypass -File eval_corpus.ps1 -Fixture <path to excel_corpus.json>

  Only the Excel instance started here is stopped, by process id.
#>
param([Parameter(Mandatory = $true)][string]$Fixture)

$ErrorActionPreference = 'Stop'
Add-Type -Namespace Loom -Name Win32 -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint processId);
'@

$utf8 = New-Object System.Text.UTF8Encoding($false)
$doc = [System.IO.File]::ReadAllText($Fixture, $utf8) | ConvertFrom-Json
$errors = @{
    -2146826281 = '#DIV/0!'; -2146826246 = '#N/A'; -2146826259 = '#NAME?'; -2146826252 = '#NUM!'
    -2146826288 = '#NULL!'; -2146826265 = '#REF!'; -2146826273 = '#VALUE!'; -2146826238 = '#CALC!'
}

$xl = New-Object -ComObject Excel.Application
$xl.Visible = $false
$xl.DisplayAlerts = $false
[uint32]$excelPid = 0
[void][Loom.Win32]::GetWindowThreadProcessId([System.IntPtr]$xl.Hwnd, [ref]$excelPid)
try {
    $wb = $xl.Workbooks.Add()
    $ws = $wb.Worksheets.Item(1)
    foreach ($p in ($doc.data.PSObject.Properties | Where-Object { $_.Name -match '^[A-Z]+[0-9]+$' })) {
        $ws.Range([string]$p.Name).Formula = [string]$p.Value
    }
    $cases = New-Object System.Collections.ArrayList
    $n = 0
    foreach ($case in $doc.cases) {
        $formula = [string]$case[0]
        $cell = $ws.Cells.Item(1 + 10 * $n, 30)
        $n++
        $kind = 'syntax'; $value = $null
        try {
            try { $cell.Formula2 = $formula } catch { $cell.Formula = $formula }
            $v = $cell.Value2
            if ($null -eq $v) { $kind = 'empty' }
            elseif ($v -is [string]) { $kind = 'text'; $value = $v }
            elseif ($v -is [bool]) { $kind = 'bool'; $value = $v }
            elseif ($v -is [int]) {
                $kind = 'err'
                $value = if ($errors.ContainsKey([int]$v)) { $errors[[int]$v] } else { "code$v" }
            }
            else { $kind = 'num'; $value = [double]$v }
        } catch { $value = $_.Exception.Message.Split("`n")[0] }
        [void]$cases.Add(@($formula, $kind, $value))
    }
    $out = [ordered]@{ _about = $doc._about; data = $doc.data; cases = $cases }
    [System.IO.File]::WriteAllText($Fixture, (ConvertTo-Json -InputObject $out -Depth 6 -Compress), $utf8)
    "Excel $($xl.Version): $n formulas evaluated"
} finally {
    try { $wb.Close($false) } catch {}
    $xl.Quit()
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($xl)
    if ($excelPid -ne 0) { Stop-Process -Id $excelPid -Force -ErrorAction SilentlyContinue }
}
