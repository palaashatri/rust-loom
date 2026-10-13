# Interoperability test for Loom Writer rich table DOCX export using Microsoft Word COM.
param(
    [string]$Path = ""
)

$ErrorActionPreference = 'Stop'

if (-not $Path) {
    $candidates = @(
        "loom-writer\crates\loom-writer-app\target\w1-table-verification.docx",
        "loom-writer\target\w1-table-verification.docx",
        "target\w1-table-verification.docx"
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) {
            $Path = (Resolve-Path $c).Path
            break
        }
    }
}

if (-not $Path -or -not (Test-Path $Path)) {
    Write-Error "DOCX file not found. Run 'cargo test -p loom-writer-app test_docx_export_contains_real_wtbl_and_shading' first."
    exit 1
}

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

$word = $null
$doc = $null

try {
    $word = New-Object -ComObject Word.Application
    $word.Visible = $false
    $word.DisplayAlerts = 0
    Write-Output "INFO  Word $($word.Version) build $($word.Build)"

    $doc = $word.Documents.Open($Path, $false, $true, $false)
    Check 'Document opens in Word read-only' ($doc -ne $null)

    Check 'Document has 1 table' ($doc.Tables.Count -eq 1) "Tables count: $($doc.Tables.Count)"

    if ($doc.Tables.Count -ge 1) {
        $tbl = $doc.Tables.Item(1)
        Check 'Table has 3 rows' ($tbl.Rows.Count -eq 3) "Rows count: $($tbl.Rows.Count)"
        Check 'Table has 3 columns' ($tbl.Columns.Count -eq 3) "Columns count: $($tbl.Columns.Count)"

        $c11 = Clean $tbl.Cell(1, 1).Range.Text
        $c12 = Clean $tbl.Cell(1, 2).Range.Text
        $c13 = Clean $tbl.Cell(1, 3).Range.Text
        Check 'Header cells: Product | Qty | Price' (($c11 -eq 'Product') -and ($c12 -eq 'Qty') -and ($c13 -eq 'Price')) "$c11 | $c12 | $c13"

        $c21 = Clean $tbl.Cell(2, 1).Range.Text
        $c22 = Clean $tbl.Cell(2, 2).Range.Text
        $c23 = Clean $tbl.Cell(2, 3).Range.Text
        Check 'Row 2 cells: Apples | 50 | $1.50' (($c21 -eq 'Apples') -and ($c22 -eq '50') -and ($c23 -eq '$1.50')) "$c21 | $c22 | $c23"

        $c31 = Clean $tbl.Cell(3, 1).Range.Text
        Check 'Merged cell in row 3 contains SpecialOffer' ($c31 -match 'SpecialOffer') "$c31"

        # Check shading on header cell or table
        $hasShd = ($tbl.Cell(1, 1).Shading.BackgroundPatternColor -ne 0) -or ($tbl.Cell(1, 1).Shading.Texture -ne 0) -or ($tbl.Rows.Item(1).Range.Font.Bold -ne 0)
        Check 'Header row has styling / bold / shading' $hasShd
    }

    $doc.Close([ref]$false)
    $doc = $null
} catch {
    $script:fails++
    Write-Output "FAIL  Unexpected Word COM error: $($_.Exception.Message)"
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

Write-Output "SUMMARY check_word_table: $script:passes passed, $script:fails failed"
if ($script:fails -gt 0) {
    exit 1
}
