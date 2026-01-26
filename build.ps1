# Build script for bsaver screensaver
# This builds the project and outputs bsaver.scr

Write-Host "Building bsaver screensaver..." -ForegroundColor Cyan

cargo build --release

if ($LASTEXITCODE -eq 0) {
    $source = "target\release\bsaver.exe"
    $dest = "bsaver.scr"
    
    Copy-Item -Path $source -Destination $dest -Force
    
    $size = [math]::Round((Get-Item $dest).Length / 1KB, 1)
    Write-Host "`nBuild successful!" -ForegroundColor Green
    Write-Host "Output: $dest ($size KB)" -ForegroundColor Green
    Write-Host "`nTo install system-wide (requires Admin):" -ForegroundColor Yellow
    Write-Host "  Copy-Item bsaver.scr `$env:SystemRoot\System32\" -ForegroundColor Gray
    Write-Host "`nOr right-click bsaver.scr and select 'Install'" -ForegroundColor Yellow
} else {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}
