# Windows CF_HTML clipboard and Word COM paste verification test.
param()

$ErrorActionPreference = 'Stop'

$script:passes = 0
$script:fails = 0

function Check([string]$name, [bool]$condition, [string]$detail = '') {
    if ($condition) {
        $script:passes++
        Write-Output "PASS  $name"
    } else {
        $script:fails++
        Write-Output "FAIL  $name  -- $detail"
    }
}

function Clean([string]$text) {
    return ($text -replace "[\r\a\u0007]", '').Trim()
}

# 1. Verify CF_HTML envelope byte offset calculations
$fragment = "<table><tr><td>ColA</td><td>ColB</td></tr><tr><td>100</td><td>200</td></tr></table>"
$headerPrefix = "Version:0.9`r`nStartHTML:00000000`r`nEndHTML:00000000`r`nStartFragment:00000000`r`nEndFragment:00000000`r`n"
$htmlOpen = "<html>`r`n<body>`r`n<!--StartFragment-->"
$htmlClose = "<!--EndFragment-->`r`n</body>`r`n</html>"

$headerLen = $headerPrefix.Length
$startHtml = $headerLen
$startFragment = $headerLen + $htmlOpen.Length
$endFragment = $startFragment + $fragment.Length
$endHtml = $endFragment + $htmlClose.Length

$cfHtml = [string]::Format(
    "Version:0.9`r`nStartHTML:{0:D8}`r`nEndHTML:{1:D8}`r`nStartFragment:{2:D8}`r`nEndFragment:{3:D8}`r`n{4}{5}{6}",
    $startHtml, $endHtml, $startFragment, $endFragment, $htmlOpen, $fragment, $htmlClose
)

$utf8Bytes = [System.Text.Encoding]::UTF8.GetBytes($cfHtml)
$actualStartFrag = [System.Text.Encoding]::UTF8.GetString($utf8Bytes, $startFragment, $fragment.Length)
Check 'CF_HTML StartFragment offset points directly to table HTML' ($actualStartFrag -eq $fragment) "$actualStartFrag"
Check 'CF_HTML StartHTML offset points to html open tag' ($cfHtml.Substring($startHtml, 6) -eq '<html>')

# 2. Put CF_HTML onto the Windows clipboard
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Clipboard]::Clear()

$dataObj = New-Object System.Windows.Forms.DataObject
$dataObj.SetData("HTML Format", $cfHtml)
$dataObj.SetData("Text", "ColA`tColB`r`n100`t200")
[System.Windows.Forms.Clipboard]::SetDataObject($dataObj, $true)

Check 'Windows clipboard contains HTML Format' ([System.Windows.Forms.Clipboard]::ContainsText([System.Windows.Forms.TextDataFormat]::Html))

# 3. Paste into Word via COM
$word = $null
$doc = $null

try {
    $word = New-Object -ComObject Word.Application
    $word.Visible = $false
    $word.DisplayAlerts = 0
    Write-Output "INFO  Word $($word.Version) build $($word.Build)"

    $doc = $word.Documents.Add()
    $word.Selection.Paste()

    Check 'Word pastes CF_HTML as a real table' ($doc.Tables.Count -ge 1) "Tables count: $($doc.Tables.Count)"

    if ($doc.Tables.Count -ge 1) {
        $tbl = $doc.Tables.Item(1)
        Check 'Pasted table has 2 rows' ($tbl.Rows.Count -eq 2) "Rows: $($tbl.Rows.Count)"
        Check 'Pasted table has 2 columns' ($tbl.Columns.Count -eq 2) "Cols: $($tbl.Columns.Count)"

        $c11 = Clean $tbl.Cell(1, 1).Range.Text
        $c12 = Clean $tbl.Cell(1, 2).Range.Text
        $c21 = Clean $tbl.Cell(2, 1).Range.Text
        $c22 = Clean $tbl.Cell(2, 2).Range.Text
        Check 'Cell values match pasted HTML table' (($c11 -eq 'ColA') -and ($c12 -eq 'ColB') -and ($c21 -eq '100') -and ($c22 -eq '200')) "$c11, $c12, $c21, $c22"
    }

    $doc.Close([ref]$false)
    $doc = $null
} catch {
    $script:fails++
    Write-Output "FAIL  Word paste COM error: $($_.Exception.Message)"
} finally {
    if ($doc) {
        try { $doc.Close([ref]$false) } catch {}
    }
    if ($word) {
        try { $word.Quit([ref]0) } catch {}
        [System.Runtime.InteropServices.Marshal]::ReleaseComObject($word) | Out-Null
    }
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
}

Write-Output "SUMMARY check_clipboard_win: $script:passes passed, $script:fails failed"
if ($script:fails -gt 0) {
    exit 1
}
