# Build TreeCraft in release mode and package a Windows zip into releases/.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $root "app\Cargo.toml"

# Version from Cargo.toml ([package] version = "x.y.z").
$version = (Select-String -Path $manifest -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
Write-Host "Building TreeCraft v$version (release)..."
cargo build --release --manifest-path $manifest
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$exe = Join-Path $root "app\target\release\treecraft.exe"
if (-not (Test-Path $exe)) { throw "missing $exe" }

$rel = Join-Path $root "releases"
New-Item -ItemType Directory -Force $rel | Out-Null
$stage = Join-Path $rel "TreeCraft-v$version-windows"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item $exe $stage
Copy-Item (Join-Path $root "README.md") $stage
Copy-Item (Join-Path $root "FEEDBACK.md") $stage -ErrorAction SilentlyContinue

$zip = Join-Path $rel "TreeCraft-v$version-windows.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path "$stage\*" -DestinationPath $zip
Remove-Item -Recurse -Force $stage
Write-Host "Packaged: $zip"
