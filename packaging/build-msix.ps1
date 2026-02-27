# build-msix.ps1
# Build the MSIX package for Windows Store submission
#
# Prerequisites:
#   - Rust toolchain (stable)
#   - Windows 10 SDK (for MakeAppx.exe and MakePri.exe)
#
# Usage:
#   .\build-msix.ps1                  # Build with auto-detected SDK
#   .\build-msix.ps1 -SkipBuild       # Package only (skip cargo build)
#   .\build-msix.ps1 -Version "1.2.0" # Override version

param(
    [string]$Version = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

# --- Configuration ---
$ProjectRoot = Split-Path -Parent $PSScriptRoot
if ($ProjectRoot -eq "") { $ProjectRoot = (Get-Location).Path }
# If running from project root directly
if (Test-Path "$PSScriptRoot\Cargo.toml") { $ProjectRoot = $PSScriptRoot }
if (Test-Path ".\Cargo.toml") { $ProjectRoot = (Get-Location).Path }

$PackagingDir = Join-Path $ProjectRoot "packaging"
$OutputDir = Join-Path $ProjectRoot "output"
$StagingDir = Join-Path $OutputDir "AppxContent"

# Read version from Cargo.toml if not specified
if ($Version -eq "") {
    $cargoContent = Get-Content (Join-Path $ProjectRoot "Cargo.toml") -Raw
    if ($cargoContent -match 'version\s*=\s*"([^"]+)"') {
        $Version = $matches[1]
    } else {
        $Version = "1.0.0"
    }
}
$MsixVersion = "$Version.0"  # MSIX needs 4-part version

Write-Host "=== BanglaSaver MSIX Builder ===" -ForegroundColor Cyan
Write-Host "Version: $MsixVersion"
Write-Host "Project: $ProjectRoot"
Write-Host ""

# --- Find Windows SDK tools ---
function Find-SdkTool($toolName) {
    # Check Windows SDK registry
    $sdkRoot = $null
    $regPaths = @(
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Microsoft SDKs\Windows\v10.0",
        "HKLM:\SOFTWARE\Microsoft\Microsoft SDKs\Windows\v10.0"
    )
    foreach ($rp in $regPaths) {
        if (Test-Path $rp) {
            $sdkRoot = (Get-ItemProperty $rp -ErrorAction SilentlyContinue).InstallationFolder
            if ($sdkRoot) { break }
        }
    }

    if (-not $sdkRoot) {
        $sdkRoot = "${env:ProgramFiles(x86)}\Windows Kits\10"
    }

    if (Test-Path $sdkRoot) {
        $binPath = Join-Path $sdkRoot "bin"
        # Find latest SDK version
        $versions = Get-ChildItem $binPath -Directory | Where-Object { $_.Name -match "^10\." } | Sort-Object Name -Descending
        foreach ($ver in $versions) {
            $tool = Join-Path $ver.FullName "x64\$toolName"
            if (Test-Path $tool) { return $tool }
        }
    }

    # Fallback: check App Certification Kit
    $ackPath = "${env:ProgramFiles(x86)}\Windows Kits\10\App Certification Kit\$toolName"
    if (Test-Path $ackPath) { return $ackPath }

    # Fallback: PATH
    $inPath = Get-Command $toolName -ErrorAction SilentlyContinue
    if ($inPath) { return $inPath.Source }

    return $null
}

$MakeAppx = Find-SdkTool "makeappx.exe"
$MakePri = Find-SdkTool "makepri.exe"

if (-not $MakeAppx) {
    Write-Error "MakeAppx.exe not found! Install Windows 10 SDK."
    exit 1
}
Write-Host "MakeAppx: $MakeAppx" -ForegroundColor Gray
if ($MakePri) {
    Write-Host "MakePri:  $MakePri" -ForegroundColor Gray
}

# --- Build Rust binaries ---
if (-not $SkipBuild) {
    Write-Host ""
    Write-Host "Building release binaries..." -ForegroundColor Yellow
    Push-Location $ProjectRoot
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Cargo build failed!"
        Pop-Location
        exit 1
    }
    Pop-Location
    Write-Host "Build complete." -ForegroundColor Green
}

# --- Stage files ---
Write-Host ""
Write-Host "Staging package content..." -ForegroundColor Yellow

if (Test-Path $StagingDir) { Remove-Item $StagingDir -Recurse -Force }
New-Item -ItemType Directory -Path $StagingDir -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $StagingDir "Assets") -Force | Out-Null

# Copy binaries
$releaseDir = Join-Path $ProjectRoot "target\release"
Copy-Item (Join-Path $releaseDir "BanglaSaver.exe") $StagingDir
Copy-Item (Join-Path $releaseDir "bsaver.exe") $StagingDir

# Copy font directory (needed by bsaver for rendering)
$fontDir = Join-Path $ProjectRoot "font"
if (Test-Path $fontDir) {
    Copy-Item $fontDir (Join-Path $StagingDir "font") -Recurse
}

# Copy assets
Copy-Item (Join-Path $PackagingDir "Assets\*") (Join-Path $StagingDir "Assets") -Force

# Copy and patch manifest with correct version (only Identity Version, not xml declaration)
$manifestContent = Get-Content (Join-Path $PackagingDir "AppxManifest.xml") -Raw
$manifestContent = $manifestContent -replace '(<Identity[^>]*)\bVersion="[^"]*"', "`$1Version=`"$MsixVersion`""
# Write without BOM - MakeAppx requires UTF-8 without BOM
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText((Join-Path $StagingDir "AppxManifest.xml"), $manifestContent, $utf8NoBom)

Write-Host "Staged files:" -ForegroundColor Gray
Get-ChildItem $StagingDir -Recurse | ForEach-Object {
    $rel = $_.FullName.Substring($StagingDir.Length + 1)
    $size = if ($_.PSIsContainer) { "<DIR>" } else { "$([math]::Round($_.Length/1KB, 1))KB" }
    Write-Host "  $rel  ($size)" -ForegroundColor Gray
}

# --- Generate resources.pri (optional but recommended) ---
if ($MakePri) {
    Write-Host ""
    Write-Host "Generating resources.pri..." -ForegroundColor Yellow
    Push-Location $StagingDir

    & $MakePri createconfig /cf priconfig.xml /dq en-US /o 2>&1 | Out-Null
    & $MakePri new /pr . /cf priconfig.xml /of resources.pri /o 2>&1 | Out-Null
    Remove-Item priconfig.xml -ErrorAction SilentlyContinue

    Pop-Location
    if (Test-Path (Join-Path $StagingDir "resources.pri")) {
        Write-Host "resources.pri generated." -ForegroundColor Green
    } else {
        Write-Host "Warning: resources.pri generation failed (non-fatal)." -ForegroundColor DarkYellow
    }
}

# --- Create MSIX package ---
Write-Host ""
Write-Host "Creating MSIX package..." -ForegroundColor Yellow

$msixPath = Join-Path $OutputDir "BanglaSaver_${MsixVersion}_x64.msix"
if (Test-Path $msixPath) { Remove-Item $msixPath -Force }

& $MakeAppx pack /d $StagingDir /p $msixPath /o /nv
if ($LASTEXITCODE -ne 0) {
    Write-Error "MakeAppx pack failed!"
    exit 1
}

$msixSize = [math]::Round((Get-Item $msixPath).Length / 1MB, 2)
Write-Host ""
Write-Host "=== MSIX Package Created ===" -ForegroundColor Green
Write-Host "Path: $msixPath"
Write-Host "Size: ${msixSize}MB"
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "  1. Test locally:  Add-AppxPackage -Path '$msixPath'" -ForegroundColor White
Write-Host "  2. Upload to Partner Center: https://partner.microsoft.com/dashboard" -ForegroundColor White
Write-Host "     Microsoft will sign the package for Store distribution." -ForegroundColor Gray
Write-Host ""
Write-Host "Note: The package is unsigned. For Store submission, upload as-is." -ForegroundColor DarkYellow
Write-Host "      Microsoft handles signing for Store-distributed packages." -ForegroundColor DarkYellow
