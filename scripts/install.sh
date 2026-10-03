#!/bin/sh
# Install enowx from the latest GitHub release (macOS and Linux).
#   curl -fsSL https://enowx.ai/install.sh | sh
# ENOWX_VERSION=v0.2.2 picks a release; ENOWX_INSTALL_DIR sets where enowx goes
# (default ~/.local/bin), and it is added to PATH in your shell's rc file
# unless ENOWX_NO_MODIFY_PATH=1.
set -eu

repo="enowdev/enowxcli"
dir="${ENOWX_INSTALL_DIR:-${ENX_INSTALL_DIR:-$HOME/.local/bin}}"

case "$(uname -s)" in
  Darwin) os=apple-darwin ;;
  Linux) os=unknown-linux-musl ;;
  *) echo "enowx: unsupported system $(uname -s); on Windows use install.ps1" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) echo "enowx: unsupported processor $(uname -m)" >&2; exit 1 ;;
esac
target="$arch-$os"

ver="${ENOWX_VERSION:-${ENX_VERSION:-}}"
if [ -n "$ver" ]; then
  url="https://github.com/$repo/releases/download/$ver/enowx-$target.tar.gz"
else
  url="https://github.com/$repo/releases/latest/download/enowx-$target.tar.gz"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "Downloading enowx for $target"
curl -fsSL "$url" -o "$tmp/enowx.tar.gz"
curl -fsSL "$url.sha256" -o "$tmp/enowx.tar.gz.sha256"
expected="$(cut -d' ' -f1 "$tmp/enowx.tar.gz.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/enowx.tar.gz" | cut -d' ' -f1)"
else
  actual="$(shasum -a 256 "$tmp/enowx.tar.gz" | cut -d' ' -f1)"
fi
[ "$expected" = "$actual" ] || { echo "enowx: checksum mismatch, not installing" >&2; exit 1; }

tar xzf "$tmp/enowx.tar.gz" -C "$tmp"
mkdir -p "$dir"
mv "$tmp/enowx-$target/enowx" "$dir/enowx"
chmod +x "$dir/enowx"
# The release binary is unsigned; an ad-hoc signature lets macOS run it.
if [ "$os" = apple-darwin ]; then
  xattr -d com.apple.quarantine "$dir/enowx" 2>/dev/null || true
  codesign -f -s - "$dir/enowx" 2>/dev/null || true
fi

version="$("$dir/enowx" --version | awk '{print $2}')"
echo "Installed enowx $version to $dir/enowx"

# Put $dir on PATH for new shells, once, in the rc file of the user's shell.
# ENX_NO_MODIFY_PATH=1 leaves the shell config alone.
case ":$PATH:" in
  *":$dir:"*) exit 0 ;;
esac
if [ -n "${ENOWX_NO_MODIFY_PATH:-${ENX_NO_MODIFY_PATH:-}}" ]; then
  echo "Add $dir to your PATH to run enowx."
  exit 0
fi
case "$(basename "${SHELL:-sh}")" in
  zsh) rc="${ZDOTDIR:-$HOME}/.zshrc"; line="export PATH=\"$dir:\$PATH\"" ;;
  bash)
    if [ "$os" = apple-darwin ]; then rc="$HOME/.bash_profile"; else rc="$HOME/.bashrc"; fi
    line="export PATH=\"$dir:\$PATH\""
    ;;
  fish) rc="$HOME/.config/fish/config.fish"; line="fish_add_path \"$dir\"" ;;
  *) rc="$HOME/.profile"; line="export PATH=\"$dir:\$PATH\"" ;;
esac
mkdir -p "$(dirname "$rc")"
if ! grep -qsF "$line" "$rc"; then
  printf '\n# enowx\n%s\n' "$line" >> "$rc"
fi
echo "Added $dir to PATH in $rc. Open a new terminal, or run: . \"$rc\""
