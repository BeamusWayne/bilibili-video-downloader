$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$releaseRoot = Join-Path $projectRoot 'release'
$version = (Get-Content -LiteralPath (Join-Path $projectRoot 'package.json') -Raw | ConvertFrom-Json).version
$portableRoot = Join-Path (Join-Path $projectRoot '.cache/portable-build') "BilibiliVideoDownloader-$version-win-x64"
$executable = Join-Path $projectRoot 'src-tauri/target/release/bilibili-video-downloader.exe'
if (-not (Test-Path -LiteralPath $executable)) { throw 'Run npm run package first.' }
New-Item -ItemType Directory -Path $portableRoot,$releaseRoot -Force | Out-Null
$portableExe = Join-Path $portableRoot 'BilibiliVideoDownloader.exe'
if (-not (Test-Path -LiteralPath $portableExe) -or (Get-FileHash -LiteralPath $portableExe).Hash -ne (Get-FileHash -LiteralPath $executable).Hash) {
  Copy-Item -LiteralPath $executable -Destination $portableExe -Force
}
$toolsDestination = Join-Path $portableRoot 'tools'
$licensesDestination = Join-Path $portableRoot 'licenses'
New-Item -ItemType Directory -Path $toolsDestination,$licensesDestination -Force | Out-Null
Get-ChildItem -LiteralPath (Join-Path $projectRoot 'src-tauri/binaries') -File | Copy-Item -Destination $toolsDestination -Force
Get-ChildItem -LiteralPath (Join-Path $projectRoot 'src-tauri/licenses') -File | Copy-Item -Destination $licensesDestination -Force
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md'),(Join-Path $projectRoot 'LICENSE'),(Join-Path $projectRoot 'THIRD_PARTY_NOTICES.md'),(Join-Path $projectRoot 'TESTING.md') -Destination $portableRoot
Copy-Item -LiteralPath (Join-Path $projectRoot 'README_EN.md'),(Join-Path $projectRoot 'CHANGELOG.md') -Destination $portableRoot
$documentationDestination = Join-Path $portableRoot 'docs'
New-Item -ItemType Directory -Path $documentationDestination -Force | Out-Null
Get-ChildItem -LiteralPath (Join-Path $projectRoot 'docs') | Copy-Item -Destination $documentationDestination -Recurse -Force
$installer = Get-ChildItem -LiteralPath (Join-Path $projectRoot 'src-tauri/target/release/bundle/nsis') -Filter "BilibiliVideoDownloader_${version}_x64-setup.exe" | Select-Object -First 1
if ($installer) { Copy-Item -LiteralPath $installer.FullName -Destination $releaseRoot }
$archive = Join-Path $releaseRoot "BilibiliVideoDownloader-$version-win-x64.zip"
Compress-Archive -Path $portableRoot -DestinationPath $archive -Force
Get-Item -LiteralPath $archive,(Join-Path $releaseRoot "BilibiliVideoDownloader_${version}_x64-setup.exe") | Get-FileHash -Algorithm SHA256 | Select-Object @{Name='File';Expression={Split-Path -Leaf $_.Path}},Hash | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $releaseRoot 'SHA256.json')
Get-ChildItem -LiteralPath $releaseRoot -File | Select-Object Name,Length
