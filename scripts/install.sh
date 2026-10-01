#!/bin/sh
# Install enx from the latest GitHub release (macOS and Linux).
#   curl -fsSL https://enowx.ai/install.sh | sh
# ENX_VERSION=v0.1.0 picks a release; ENX_INSTALL_DIR sets where enx goes
# (default ~/.local/bin), and it is added to PATH in your shell's rc file
# unless ENX_NO_MODIFY_PATH=1. A finished install is counted on enowx.ai
# (build and version only) unless ENX_NO_STATS=1.
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

version="$("$dir/enx" --version | awk '{print $2}')"
echo "Installed enx $version to $dir/enx"

# Count the install on enowx.ai: the build and version, nothing else. It
# never holds up or fails the install. ENX_NO_STATS=1 skips it.
if [ -z "${ENX_NO_STATS:-}" ]; then
  curl -fsS -m 5 -X POST -H "Content-Type: application/json" \
    -d "{\"target\":\"$target\",\"version\":\"$version\"}" \
    https://enowx.ai/api/installs >/dev/null 2>&1 || true
fi

# Put $dir on PATH for new shells, once, in the rc file of the user's shell.
# ENX_NO_MODIFY_PATH=1 leaves the shell config alone.
case ":$PATH:" in
  *":$dir:"*) exit 0 ;;
esac
if [ -n "${ENX_NO_MODIFY_PATH:-}" ]; then
  echo "Add $dir to your PATH to run enx."
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
  printf '\n# enx\n%s\n' "$line" >> "$rc"
fi
echo "Added $dir to PATH in $rc. Open a new terminal, or run: . \"$rc\""
