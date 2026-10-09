# Generate GreenPool icons (no external CLI): 32/128/256 PNG + icon.ico from a solid brand-color canvas with "GP".
# ASCII-only, run: powershell -NoProfile -ExecutionPolicy Bypass -File gen_icon.ps1
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root = 'c:\Users\nstl2\CodeBuddy\20260925153614\greenpool'
$icons = Join-Path $root 'src-tauri\icons'
New-Item -ItemType Directory -Force $icons | Out-Null

function New-Canvas([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $g.Clear([System.Drawing.ColorTranslator]::FromHtml('#0B6FA4'))
    $font = New-Object System.Drawing.Font 'Segoe UI', ([math]::Round($size * 0.42)), ([System.Drawing.FontStyle]::Bold), ([System.Drawing.GraphicsUnit]::Pixel)
    $sf = New-Object System.Drawing.StringFormat
    $sf.Alignment = [System.Drawing.StringAlignment]::Center
    $sf.LineAlignment = [System.Drawing.StringAlignment]::Center
    $rect = New-Object System.Drawing.RectangleF 0, 0, $size, $size
    $g.DrawString('GP', $font, [System.Drawing.Brushes]::White, $rect, $sf)
    return @($bmp, $g)
}

foreach ($pair in @(@(32, '32x32.png'), @(128, '128x128.png'), @(256, '128x128@2x.png'))) {
    $size = $pair[0]; $name = $pair[1]
    $bmp, $g = New-Canvas $size
    $bmp.Save((Join-Path $icons $name), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Host ("wrote " + $name)
}

$bmp, $g = New-Canvas 256
$hIcon = $bmp.GetHicon()
$icon = [System.Drawing.Icon]::FromHandle($hIcon)
$fs = [System.IO.File]::Create((Join-Path $icons 'icon.ico'))
$icon.Save($fs)
$fs.Close()
$icon.Dispose()
$g.Dispose(); $bmp.Dispose()
Write-Host 'wrote icon.ico'
