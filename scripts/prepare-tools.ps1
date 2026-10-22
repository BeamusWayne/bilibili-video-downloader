$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$binaryDir = Join-Path $projectRoot 'src-tauri/binaries'
$cacheDir = Join-Path $projectRoot '.cache'
$licenseDir = Join-Path $projectRoot 'src-tauri/licenses'
New-Item -ItemType Directory -Path $binaryDir,$cacheDir,$licenseDir -Force | Out-Null
if (-not (Test-Path "$binaryDir/yt-dlp.exe")) {
  $release = Invoke-RestMethod 'https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest'
  $asset = $release.assets | Where-Object name -eq 'yt-dlp.exe'
  Invoke-WebRequest $asset.browser_download_url -OutFile "$cacheDir/yt-dlp.exe"
  $hashes = (Invoke-WebRequest ($release.assets | Where-Object name -eq 'SHA2-256SUMS').browser_download_url).Content
  if ($hashes -is [byte[]]) { $hashes = [Text.Encoding]::UTF8.GetString($hashes) }
  $expected = (($hashes -split "`n" | Where-Object { $_ -match '\syt-dlp.exe$' }) -split '\s+')[0]
  if ((Get-FileHash "$cacheDir/yt-dlp.exe" -Algorithm SHA256).Hash.ToLower() -ne $expected.ToLower()) { throw 'yt-dlp checksum mismatch' }
  Copy-Item "$cacheDir/yt-dlp.exe" "$binaryDir/yt-dlp.exe"
  Invoke-WebRequest 'https://raw.githubusercontent.com/yt-dlp/yt-dlp/master/LICENSE' -OutFile "$licenseDir/yt-dlp-LICENSE.txt"
  $release.tag_name | Set-Content "$licenseDir/yt-dlp-version.txt"
}
if (-not (Test-Path "$binaryDir/ffmpeg.exe")) {
  $url = 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip'
  Invoke-WebRequest $url -OutFile "$cacheDir/ffmpeg.zip"
  $hashText = (Invoke-WebRequest "$url.sha256").Content
  if ($hashText -is [byte[]]) { $hashText = [Text.Encoding]::UTF8.GetString($hashText) }
  $expected = ($hashText.Trim() -split '\s+')[0]
  if ((Get-FileHash "$cacheDir/ffmpeg.zip" -Algorithm SHA256).Hash.ToLower() -ne $expected.ToLower()) { throw 'FFmpeg checksum mismatch' }
  Expand-Archive -LiteralPath "$cacheDir/ffmpeg.zip" -DestinationPath "$cacheDir/ffmpeg" -Force
  $ffmpegRoot = Get-ChildItem "$cacheDir/ffmpeg" -Directory | Select-Object -First 1
  Copy-Item "$($ffmpegRoot.FullName)/bin/ffmpeg.exe","$($ffmpegRoot.FullName)/bin/ffprobe.exe" $binaryDir
  Copy-Item "$($ffmpegRoot.FullName)/LICENSE" "$licenseDir/FFmpeg-LICENSE.txt"
  Copy-Item "$($ffmpegRoot.FullName)/README.txt" "$licenseDir/FFmpeg-README.txt"
}
& "$binaryDir/ffmpeg.exe" -version | Select-Object -First 1
& "$binaryDir/yt-dlp.exe" --version
Get-FileHash "$binaryDir/*.exe" -Algorithm SHA256 | Select-Object @{Name='File';Expression={Split-Path -Leaf $_.Path}},Hash | ConvertTo-Json | Set-Content "$licenseDir/tool-checksums.json"
