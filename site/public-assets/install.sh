#!/bin/sh
# Installs the hitop command line (https://hitop.rws8.tech) from GitHub Releases:
#
#   curl -fsSL https://hitop.rws8.tech/install.sh | sh
#
# Linux (static musl build) and macOS, x86_64 and arm64. The download's SHA-256 is checked
# against the .sha256 published next to it. No root needed: it installs to ~/.local/bin unless
# you say otherwise.
#
#   HITOP_VERSION=1.2.0        a version instead of the newest release
#   HITOP_INSTALL_DIR=/usr/local/bin   where to put the binary (default ~/.local/bin)
#   GITHUB_TOKEN=...           optional, for the release lookup in CI (rate limit)
#
# Windows: download hitop-x86_64-pc-windows-msvc.zip from
# https://github.com/bohdaq/hitop/releases (or use WSL with this script).
set -eu

REPO="bohdaq/hitop"
DIR="${HITOP_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf 'hitop install: %s\n' "$*" >&2; }
fail() { say "$*"; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || fail "needs $1"; }
need uname
need tar
need mkdir

# GITHUB_TOKEN, when set (CI), lifts GitHub's anonymous API rate limit for the release lookup.
if command -v curl >/dev/null 2>&1; then
  get() {
    if [ -n "${GITHUB_TOKEN:-}" ]; then curl -fsSL --retry 3 -H "Authorization: Bearer $GITHUB_TOKEN" "$1"; else curl -fsSL --retry 3 "$1"; fi
  }
  get_to() { curl -fsSL --retry 3 -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  get() { wget -qO- "$1"; }
  get_to() { wget -qO "$2" "$1"; }
else
  fail "needs curl or wget"
fi

os=$(uname -s)
arch=$(uname -m)
case "$arch" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) fail "no build for $arch yet; install with cargo install hitop" ;;
esac
case "$os" in
  Linux) target="$arch-unknown-linux-musl" ;;
  Darwin) target="$arch-apple-darwin" ;;
  MINGW* | MSYS* | CYGWIN*) fail "on Windows, download hitop-x86_64-pc-windows-msvc.zip from https://github.com/$REPO/releases" ;;
  *) fail "no build for $os yet; install with cargo install hitop" ;;
esac

if [ -n "${HITOP_VERSION:-}" ]; then
  tag="cli-v${HITOP_VERSION#v}"
else
  # The newest CLI release (desktop releases share the repository; release candidates are skipped).
  tag=$(get "https://api.github.com/repos/$REPO/releases?per_page=30" \
    | grep -o '"tag_name": *"cli-v[0-9][0-9.]*"' | head -n 1 | sed 's/.*"\(cli-v[^"]*\)"/\1/') \
    || true
  [ -n "$tag" ] || fail "couldn't find the newest release (GitHub API); set HITOP_VERSION=1.2.0 and run again"
fi

asset="hitop-$target.tar.gz"
url="https://github.com/$REPO/releases/download/$tag/$asset"
tmp=$(mktemp -d 2>/dev/null || mktemp -d -t hitop)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading ${tag#cli-v} for $target"
get_to "$url" "$tmp/$asset" || fail "download failed: $url"
get_to "$url.sha256" "$tmp/$asset.sha256" || fail "download failed: $url.sha256"

want=$(awk '{print $1}' "$tmp/$asset.sha256")
if command -v sha256sum >/dev/null 2>&1; then
  got=$(sha256sum "$tmp/$asset" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
  got=$(shasum -a 256 "$tmp/$asset" | awk '{print $1}')
else
  fail "needs sha256sum or shasum to check the download"
fi
[ "$got" = "$want" ] || fail "checksum mismatch for $asset (expected $want, got $got)"

tar -xzf "$tmp/$asset" -C "$tmp"
bin=$(find "$tmp" -type f -name hitop | head -n 1)
[ -n "$bin" ] || fail "no hitop binary in $asset"

mkdir -p "$DIR"
if [ -w "$DIR" ]; then
  cp "$bin" "$DIR/hitop.new" && chmod 755 "$DIR/hitop.new" && mv "$DIR/hitop.new" "$DIR/hitop"
elif command -v sudo >/dev/null 2>&1; then
  say "$DIR isn't writable: using sudo"
  sudo install -m 755 "$bin" "$DIR/hitop"
else
  fail "$DIR isn't writable; set HITOP_INSTALL_DIR to a folder you own"
fi

say "installed $("$DIR/hitop" --version 2>/dev/null || echo hitop) to $DIR/hitop"
case ":$PATH:" in
  *":$DIR:"*) ;;
  *) say "add $DIR to your PATH, e.g. echo 'export PATH=\"$DIR:\$PATH\"' >> ~/.profile" ;;
esac
