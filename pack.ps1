# pack.ps1 - Automated Release Builder & PE Overlay Packager for ClipShrinker
param(
    [string]$BuildNum = "5"
)

$ErrorActionPreference = "Stop"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "  Building ClipShrinker with PE Overlay architecture" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

# 0. Source code archive discipline
$verTag = "9.0.2"
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$archiveDir = "archives"
if (-not (Test-Path $archiveDir)) {
    New-Item -ItemType Directory -Path $archiveDir | Out-Null
}
$archiveZip = "$archiveDir\src_v${verTag}_${timestamp}.zip"
Write-Host "`n[1/4] Archiving source snapshot: $archiveZip..." -ForegroundColor Yellow

$filesToArchive = Get-ChildItem -Path . -Exclude "target", "archives", "bin", "*.exe", "*.xz", "*.zip", "ffmpeg.*", ".git" | Select-Object -ExpandProperty FullName
Compress-Archive -Path $filesToArchive -DestinationPath $archiveZip -Force
Write-Host "  Source archive saved ($([math]::Round((Get-Item $archiveZip).Length / 1024, 1)) KB)" -ForegroundColor Green

# 1. Cargo build release
Write-Host "`n[2/4] Compiling release PE stub (Cargo)..." -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo release build failed"
    exit 1
}

$stubPath = "target\release\clip_shrinker.exe"
$stubSize = (Get-Item $stubPath).Length
$stubKb = [math]::Round($stubSize / 1024, 1)
Write-Host "  PE-stub compiled: $stubPath ($stubKb KB)" -ForegroundColor Green

# 2. Check payload
$payloadPath = "ffmpeg.xz"
if (-not (Test-Path $payloadPath)) {
    if (Test-Path "ffmpeg.exe") {
        Write-Host "  Compressing ffmpeg.exe -> ffmpeg.xz..." -ForegroundColor Yellow
        $payloadPath = "ffmpeg.exe"
    } else {
        Write-Error "Neither ffmpeg.xz nor ffmpeg.exe found for overlay packaging!"
        exit 1
    }
}

# 3. Pack overlay
Write-Host "`n[3/4] Packaging PE overlay and creating standalone .exe..." -ForegroundColor Yellow
$packCmd = "$stubPath --pack-overlay $stubPath $payloadPath ClipShrinker.exe $BuildNum"
cmd /c $packCmd
if ($LASTEXITCODE -ne 0) {
    Write-Error "Overlay packaging failed"
    exit 1
}

$finalSize = (Get-Item "ClipShrinker.exe").Length
$finalMb = [math]::Round($finalSize / 1048576, 2)
Write-Host "  Primary executable: ClipShrinker.exe ($finalMb MB)" -ForegroundColor Green

# 4. Copy versioned artifact into bin/
$binDir = "bin"
if (-not (Test-Path $binDir)) {
    New-Item -ItemType Directory -Path $binDir | Out-Null
}
$verOutput = (cmd /c ".\ClipShrinker.exe --get-version").Trim()
$versionedName = "$binDir\clip_shrinker_v${verOutput}_win64.exe"
Copy-Item "ClipShrinker.exe" $versionedName -Force
Write-Host "`n[4/4] Created distribution artifact: $versionedName" -ForegroundColor Green

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "  Build completed successfully: $versionedName" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
