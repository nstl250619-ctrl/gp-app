# design-gate (Windows) - ASCII only.
# Checks:
#  1) no hardcoded hex colors outside design-tokens.css (allow #fff/#000)
#  2) no CJK string literals inside .tsx components (copy must live in src/i18n)
#  3) no emoji inside .tsx/.ts source
#  4) <= 300 effective lines per file
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root 'src'
$script:fail = 0
function Fail([string]$m) { Write-Host "[FAIL] $m" -ForegroundColor Red; $script:fail++ }

# 1) hardcoded hex colors
$files = Get-ChildItem $src -Recurse -Include *.css,*.tsx,*.ts |
    Where-Object { $_.FullName -notmatch 'design-tokens' }
foreach ($f in $files) {
    $hits = Select-String -Path $f.FullName -Pattern '#[0-9a-fA-F]{3,8}' -AllMatches
    foreach ($h in $hits) {
        if ($h.Line -notmatch '#fff|#000|#ffffff|#000000') {
            Fail "hardcoded color in $($f.Name):$($h.LineNumber)"
        }
    }
}

# 2) CJK in .tsx (UI copy), skipping comment-only lines
$tsx = Get-ChildItem $src -Recurse -Include *.tsx
foreach ($f in $tsx) {
    $hits = Select-String -Path $f.FullName -Pattern '[\u4e00-\u9fff]'
    foreach ($h in $hits) {
        $trimmed = $h.Line.TrimStart()
        if (-not ($trimmed.StartsWith('//') -or $trimmed.StartsWith('*'))) {
            Fail "CJK literal in $($f.Name):$($h.LineNumber)"
        }
    }
}

# 3) emoji
foreach ($f in (Get-ChildItem $src -Recurse -Include *.tsx,*.ts)) {
    $hits = Select-String -Path $f.FullName -Pattern '[\uD83C-\uDBFF\uDC00-\uDFFF]'
    foreach ($h in $hits) { Fail "emoji in $($f.Name):$($h.LineNumber)" }
}

# 4) effective lines <= 300
foreach ($f in (Get-ChildItem $src -Recurse -Include *.tsx,*.ts)) {
    $effective = (Get-Content $f.FullName | Where-Object { $_ -notmatch '^\s*(//|$)' }).Count
    if ($effective -gt 300) { Fail "too many effective lines ($effective) in $($f.Name)" }
}

if ($script:fail -gt 0) {
    Write-Host "design-gate FAILED ($($script:fail) issue(s))" -ForegroundColor Red
    exit 1
}
Write-Host 'design-gate passed'
