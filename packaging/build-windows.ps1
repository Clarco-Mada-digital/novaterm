# Construit NovaTerm pour Windows dans dist\ :
#   NovaTerm-<version>-windows-x64.zip      (portable : décompresser et lancer novaterm.exe)
#   NovaTerm-<version>-setup-x64.exe        (installateur, si Inno Setup est installé)
#
# Prérequis : Rust (https://rustup.rs) ; facultatif : Inno Setup 6 (choco install innosetup).
# Usage (PowerShell) : .\packaging\build-windows.ps1
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches.Groups[1].Value
$dist = Join-Path $PWD "dist"
$work = Join-Path $PWD "target\package-windows"
New-Item -ItemType Directory -Force -Path $dist, $work | Out-Null

Write-Host "==> Compilation release ($version)" -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build a échoué" }
$exe = Join-Path $PWD "target\release\novaterm.exe"

$ico = Join-Path $PWD "assets\icon\novaterm.ico"

Write-Host "==> Archive portable" -ForegroundColor Cyan
$portable = Join-Path $work "NovaTerm"
New-Item -ItemType Directory -Force -Path $portable | Out-Null
Copy-Item $exe $portable
Copy-Item README.md $portable
Copy-Item $ico $portable
$zip = Join-Path $dist "NovaTerm-$version-windows-x64.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path "$portable\*" -DestinationPath $zip
Write-Host "  -> $zip"

$iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
    $candidate = Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"
    if (Test-Path $candidate) { $iscc = $candidate }
}
if ($iscc) {
    Write-Host "==> Installateur (Inno Setup)" -ForegroundColor Cyan
    & $iscc "/DAppVersion=$version" "/DSourceDir=$portable" "/DOutputDir=$dist" "packaging\windows\novaterm.iss"
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup a échoué ($LASTEXITCODE)" }
    Write-Host "  -> $dist\NovaTerm-$version-setup-x64.exe"
} else {
    Write-Host "  (Inno Setup absent : installateur ignoré — choco install innosetup)" -ForegroundColor Yellow
}
Get-ChildItem $dist
