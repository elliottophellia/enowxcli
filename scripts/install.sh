#!/bin/sh
# Install enx from the latest GitHub release (macOS and Linux).
#   curl -fsSL https://raw.githubusercontent.com/enowdev/enowxcli/main/scripts/install.sh | sh
# ENX_VERSION=v0.1.0 picks a release; ENX_INSTALL_DIR sets where enx goes
# (default ~/.local/bin).
set -eu

repo="enowdev/enowxcli"
dir="${ENX_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
  Darwin) os=apple-darwin ;;
  Linux) os=unknown-linux-musl ;;
  *) echo "enx: unsupported system $(uname -s); on Windows use install.ps1" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) echo "enx: unsupported processor $(uname -m)" >&2; exit 1 ;;
esac
target="$arch-$os"

if [ -n "${ENX_VERSION:-}" ]; then
  url="https://github.com/$repo/releases/download/$ENX_VERSION/enx-$target.tar.gz"
else
  url="https://github.com/$repo/releases/latest/download/enx-$target.tar.gz"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "Downloading enx for $target"
curl -fsSL "$url" -o "$tmp/enx.tar.gz"
curl -fsSL "$url.sha256" -o "$tmp/enx.tar.gz.sha256"
expected="$(cut -d' ' -f1 "$tmp/enx.tar.gz.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/enx.tar.gz" | cut -d' ' -f1)"
else
  actual="$(shasum -a 256 "$tmp/enx.tar.gz" | cut -d' ' -f1)"
fi
[ "$expected" = "$actual" ] || { echo "enx: checksum mismatch, not installing" >&2; exit 1; }

tar xzf "$tmp/enx.tar.gz" -C "$tmp"
mkdir -p "$dir"
mv "$tmp/enx-$target/enx" "$dir/enx"
chmod +x "$dir/enx"
# The release binary is unsigned; an ad-hoc signature lets macOS run it.
if [ "$os" = apple-darwin ]; then
  xattr -d com.apple.quarantine "$dir/enx" 2>/dev/null || true
  codesign -f -s - "$dir/enx" 2>/dev/null || true
fi

echo "Installed $("$dir/enx" --version) to $dir/enx"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "Add $dir to your PATH, e.g. echo 'export PATH=\"$dir:\$PATH\"' >> ~/.zshrc" ;;
esac
