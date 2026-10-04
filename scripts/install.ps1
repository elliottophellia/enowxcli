# Install enowx from the latest GitHub release (Windows).
#   irm https://enowx.ai/install.ps1 | iex
# $env:ENOWX_VERSION = "v0.2.2" picks a release; $env:ENOWX_INSTALL_DIR sets where
# enowx goes (default %LOCALAPPDATA%\Programs\enowx). The command answers to two
# names, enowx and enx (enx.exe is a copy of enowx.exe), and an older enx.exe or
# enowx.exe found on the PATH is replaced with this one. $env:ENOWX_UNINSTALL = 1
# removes both; your settings and sessions in ~\.enx are never touched.
$ErrorActionPreference = "Stop"

$repo = "enowdev/enowxcli"
$dir = if ($env:ENOWX_INSTALL_DIR) { $env:ENOWX_INSTALL_DIR } elseif ($env:ENX_INSTALL_DIR) { $env:ENX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\enowx" }

# Whether a program is enowx (any version, under either name), going by what
# it says it is. Nothing else named enx is ever touched.
function Test-Enowx($path) {
  try {
    $line = (& $path --version 2>$null | Select-Object -First 1)
    return ($line -like "enowx *") -or ($line -like "enx *")
  } catch { return $false }
}

if ($env:ENOWX_UNINSTALL) {
  foreach ($name in @("enowx.exe", "enx.exe")) {
    $path = Join-Path $dir $name
    if (Test-Path $path) { Remove-Item -Force $path; Write-Host "Removed $path" }
  }
  Write-Host "Your settings and sessions in ~\.enx are kept; delete that folder yourself if you want them gone."
  return
}
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
  $fresh = Join-Path $tmp "enowx-$target\enowx.exe"
  # A running .exe cannot be overwritten, but can be moved aside.
  foreach ($name in @("enowx", "enx")) {
    $path = Join-Path $dir "$name.exe"
    if (Test-Path $path) {
      $old = Join-Path $dir "$name.old.exe"
      Remove-Item -Force $old -ErrorAction SilentlyContinue
      Move-Item -Force $path $old
    }
    Copy-Item $fresh $path -Force
  }
} finally {
  Remove-Item -Recurse -Force $tmp
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($userPath -split ";") -notcontains $dir) {
  [Environment]::SetEnvironmentVariable("Path", "$userPath;$dir", "User")
  Write-Host "Added $dir to your PATH; open a new terminal to use enowx."
}
$version = ((& (Join-Path $dir "enowx.exe") --version) -split " ")[1]
Write-Host "Installed enowx $version to $dir\enowx.exe (also runs as enx)"

# An older enowx elsewhere on the PATH (as enx.exe or enowx.exe) would still
# run in a shell that finds it first: replace it with this one.
$here = (Resolve-Path $dir).Path.TrimEnd("\")
foreach ($name in @("enx", "enowx")) {
  foreach ($found in @(Get-Command "$name.exe" -All -CommandType Application -ErrorAction SilentlyContinue)) {
    $path = $found.Source
    if ((Split-Path $path -Parent).TrimEnd("\") -eq $here) { continue }
    if (-not (Test-Enowx $path)) { continue }
    try {
      $old = [IO.Path]::ChangeExtension($path, "old.exe")
      Remove-Item -Force $old -ErrorAction SilentlyContinue
      Move-Item -Force $path $old
      Copy-Item (Join-Path $dir "enowx.exe") $path -Force
      Write-Host "Replaced the older $path with this enowx"
    } catch {
      Write-Host "Could not replace the older $path ($($_.Exception.Message)); remove it, or put $dir first on your PATH."
    }
  }
}

