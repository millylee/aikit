#!/usr/bin/env bash
set -euo pipefail

REPO="${AIKIT_REPO:-millylee/aikit}"
VERSION="${AIKIT_VERSION:-latest}"
BIN_DIR="${AIKIT_BIN_DIR:-$HOME/.local/bin}"

path_contains() {
  local entry="$1"
  case ":${PATH:-}:" in
    *":${entry}:"*) return 0 ;;
    *) return 1 ;;
  esac
}

profile_file() {
  if [ -n "${AIKIT_SHELL_PROFILE:-}" ]; then
    printf '%s\n' "$AIKIT_SHELL_PROFILE"
    return
  fi

  case "${SHELL:-}" in
    */zsh) printf '%s\n' "$HOME/.zshrc" ;;
    */bash)
      if [ "$(uname -s)" = "Darwin" ]; then
        printf '%s\n' "$HOME/.bash_profile"
      else
        printf '%s\n' "$HOME/.bashrc"
      fi
      ;;
    */fish) printf '%s\n' "$HOME/.config/fish/config.fish" ;;
    *) printf '%s\n' "$HOME/.profile" ;;
  esac
}

add_path_to_profile() {
  local bin_dir="$1"
  local profile="$2"

  mkdir -p "$(dirname "$profile")"
  touch "$profile"

  if grep -F "$bin_dir" "$profile" >/dev/null 2>&1; then
    return
  fi

  case "$profile" in
    */config.fish)
      {
        printf '\n# aikit\n'
        printf 'fish_add_path %s\n' "$bin_dir"
      } >>"$profile"
      ;;
    *)
      {
        printf '\n# aikit\n'
        printf "export PATH=\"%s:\$PATH\"\n" "$bin_dir"
      } >>"$profile"
      ;;
  esac
}

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

if path_contains "$BIN_DIR"; then
  exit 0
fi

shell_profile="$(profile_file)"
add_path_to_profile "$BIN_DIR" "$shell_profile"

echo "Added ${BIN_DIR} to PATH in ${shell_profile}."
echo "Run this in the current terminal to use aikit immediately:"
echo "  export PATH=\"${BIN_DIR}:\$PATH\""
