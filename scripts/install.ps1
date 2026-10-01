# Install enx from the latest GitHub release (Windows).
#   irm https://enowx.ai/install.ps1 | iex
# $env:ENX_VERSION = "v0.1.0" picks a release; $env:ENX_INSTALL_DIR sets where
# enx goes (default %LOCALAPPDATA%\Programs\enx).
$ErrorActionPreference = "Stop"

$repo = "enowdev/enowxcli"
$dir = if ($env:ENX_INSTALL_DIR) { $env:ENX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\enx" }
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
$target = "$arch-pc-windows-msvc"
$url = if ($env:ENX_VERSION) {
  "https://github.com/$repo/releases/download/$($env:ENX_VERSION)/enx-$target.zip"
} else {
  "https://github.com/$repo/releases/latest/download/enx-$target.zip"
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid())
New-Item -ItemType Directory $tmp | Out-Null
try {
  Write-Host "Downloading enx for $target"
  $zip = Join-Path $tmp "enx.zip"
  Invoke-WebRequest $url -OutFile $zip -UseBasicParsing
  Invoke-WebRequest "$url.sha256" -OutFile "$zip.sha256" -UseBasicParsing
  $expected = (Get-Content "$zip.sha256").Split(" ")[0].Trim()
  $actual = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
  if ($expected -ne $actual) { throw "checksum mismatch, not installing" }

  Expand-Archive $zip -DestinationPath $tmp
  New-Item -ItemType Directory -Force $dir | Out-Null
  Copy-Item (Join-Path $tmp "enx-$target\enx.exe") (Join-Path $dir "enx.exe") -Force
} finally {
  Remove-Item -Recurse -Force $tmp
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($userPath -split ";") -notcontains $dir) {
  [Environment]::SetEnvironmentVariable("Path", "$userPath;$dir", "User")
  Write-Host "Added $dir to your PATH; open a new terminal to use enx."
}
$version = ((& (Join-Path $dir "enx.exe") --version) -split " ")[1]
Write-Host "Installed enx $version to $dir\enx.exe"

