#!/usr/bin/env sh
set -eu

REPO="${AIKIT_REPO:-aikit-rs/aikit}"
VERSION="${AIKIT_VERSION:-latest}"
BIN_DIR="${AIKIT_BIN_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch_name="$(uname -m)"

case "$os" in
  Linux)
    platform="unknown-linux-gnu"
    ;;
  Darwin)
    platform="apple-darwin"
    ;;
  *)
    echo "Unsupported operating system: $os" >&2
    exit 1
    ;;
esac

case "$arch_name" in
  x86_64 | amd64)
    arch="x86_64"
    ;;
  arm64 | aarch64)
    arch="aarch64"
    ;;
  *)
    echo "Unsupported architecture: $arch_name" >&2
    exit 1
    ;;
esac

if [ "$platform" = "unknown-linux-gnu" ] && [ "$arch" != "x86_64" ]; then
  echo "Unsupported release target: ${arch}-${platform}" >&2
  exit 1
fi

archive="aikit-${arch}-${platform}.tar.gz"

if [ "$VERSION" = "latest" ]; then
  url="https://github.com/${REPO}/releases/latest/download/${archive}"
else
  url="https://github.com/${REPO}/releases/download/${VERSION}/${archive}"
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/aikit.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

archive_path="${tmp_dir}/${archive}"

if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$url" -o "$archive_path"
elif command -v wget >/dev/null 2>&1; then
  wget -qO "$archive_path" "$url"
else
  echo "Either curl or wget is required to download aikit." >&2
  exit 1
fi

tar -xzf "$archive_path" -C "$tmp_dir"

mkdir -p "$BIN_DIR"
cp "${tmp_dir}/aikit" "${BIN_DIR}/aikit"
chmod +x "${BIN_DIR}/aikit"

echo "Installed aikit to ${BIN_DIR}/aikit"

case ":${PATH:-}:" in
  *":${BIN_DIR}:"*) ;;
  *)
    echo "Add ${BIN_DIR} to your PATH to run aikit from any directory."
    ;;
esac
