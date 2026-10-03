# Install enowx from the latest GitHub release (Windows).
#   irm https://enowx.ai/install.ps1 | iex
# $env:ENOWX_VERSION = "v0.2.2" picks a release; $env:ENOWX_INSTALL_DIR sets where
# enowx goes (default %LOCALAPPDATA%\Programs\enowx).
$ErrorActionPreference = "Stop"

$repo = "enowdev/enowxcli"
$dir = if ($env:ENOWX_INSTALL_DIR) { $env:ENOWX_INSTALL_DIR } elseif ($env:ENX_INSTALL_DIR) { $env:ENX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\enowx" }
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
$target = "$arch-pc-windows-msvc"
$ver = if ($env:ENOWX_VERSION) { $env:ENOWX_VERSION } elseif ($env:ENX_VERSION) { $env:ENX_VERSION } else { "" }
$url = if ($ver) {
  "https://github.com/$repo/releases/download/$ver/enowx-$target.zip"
} else {
  "https://github.com/$repo/releases/latest/download/enowx-$target.zip"
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid())
New-Item -ItemType Directory $tmp | Out-Null
try {
  Write-Host "Downloading enowx for $target"
  $zip = Join-Path $tmp "enowx.zip"
  Invoke-WebRequest $url -OutFile $zip -UseBasicParsing
  Invoke-WebRequest "$url.sha256" -OutFile "$zip.sha256" -UseBasicParsing
  $expected = (Get-Content "$zip.sha256").Split(" ")[0].Trim()
  $actual = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
  if ($expected -ne $actual) { throw "checksum mismatch, not installing" }

  Expand-Archive $zip -DestinationPath $tmp
  New-Item -ItemType Directory -Force $dir | Out-Null
  Copy-Item (Join-Path $tmp "enowx-$target\enowx.exe") (Join-Path $dir "enowx.exe") -Force
} finally {
  Remove-Item -Recurse -Force $tmp
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($userPath -split ";") -notcontains $dir) {
  [Environment]::SetEnvironmentVariable("Path", "$userPath;$dir", "User")
  Write-Host "Added $dir to your PATH; open a new terminal to use enowx."
}
$version = ((& (Join-Path $dir "enowx.exe") --version) -split " ")[1]
Write-Host "Installed enowx $version to $dir\enowx.exe"

