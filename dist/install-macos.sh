#!/usr/bin/env bash
#
# Aoxn macOS installer (unreleased) — build from source, install to a prefix.
#
# macOS has no single-file packaged installer yet (the Windows artifact is a
# PE stub with an appended archive). This script is the native equivalent: it
# builds the compiler with cargo, lays the toolchain out exactly the way the
# compiler discovers it at runtime (src/paths.rs) —
#
#   <prefix>/bin/aoxn          the compiler
#   <prefix>/lib/stdlib/...    the WHOLE stdlib tree (incl. net/, openai/, ...)
#   <prefix>/examples/...      demo programs
#   <prefix>/docs/install.md   LICENSE  README.md  CHANGELOG.md
#
# — puts <prefix>/bin on PATH, and runs `aoxn doctor`. The install root is the
# parent of bin/, so AOXN_HOME does not need to be set.
#
# It downloads NOTHING. clang (Apple Command Line Tools) and rustc (Rust) are
# prerequisites; if one is missing the script says how to install it and stops.
#
# Usage, from the repository root:
#
#   bash dist/install-macos.sh                 # installs to ~/.local/aoxn
#   bash dist/install-macos.sh --prefix /opt/aoxn
#   bash dist/install-macos.sh --skip-build    # reuse target/release/aoxn
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="$HOME/.local/aoxn"
SKIP_BUILD=0

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    --prefix=*) PREFIX="${1#--prefix=}"; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h|--help)
      grep '^#' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

cd "$ROOT"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "install-macos.sh: this installer is for macOS (found $(uname -s))." >&2
  echo "On Windows use dist/package.ps1; on Linux build with cargo and copy the" >&2
  echo "same bin/ + lib/stdlib layout to a prefix of your choice." >&2
  exit 1
fi

VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
echo "==> Aoxn v${VERSION} (macOS)"

# ---- prerequisites (report, never download) --------------------------------
if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found. Install Rust first: https://rustup.rs" >&2
  exit 1
fi
if ! command -v clang >/dev/null 2>&1; then
  echo "error: clang not found. Install the Apple Command Line Tools:" >&2
  echo "    xcode-select --install" >&2
  exit 1
fi
echo "==> cargo: $(cargo --version)"
echo "==> clang: $(clang --version | head -1)"

# ---- build ------------------------------------------------------------------
if [ "$SKIP_BUILD" -eq 0 ]; then
  echo "==> building (release)"
  cargo build --release --locked --bin aoxn
fi
COMPILER="target/release/aoxn"
if [ ! -f "$COMPILER" ]; then
  echo "error: missing $COMPILER (run without --skip-build)" >&2
  exit 1
fi

# ---- lay out the toolchain --------------------------------------------------
echo "==> installing to $PREFIX"
mkdir -p "$PREFIX/bin" "$PREFIX/lib" "$PREFIX/examples" "$PREFIX/docs"
install -m 0755 "$COMPILER" "$PREFIX/bin/aoxn"

# the WHOLE stdlib tree (the Windows package only ships top-level *.ax; macOS
# ships the SDK + transport subdirectories too, so net/http_macos works)
rm -rf "$PREFIX/lib/stdlib"
cp -R stdlib "$PREFIX/lib/stdlib"
# the Rust bridges are source the compiler scans/compiles at build time; keep
# them beside the .ax modules they back (they are part of the stdlib tree).

cp -R examples/. "$PREFIX/examples/"
[ -f docs/install.md ] && install -m 0644 docs/install.md "$PREFIX/docs/install.md"
for f in LICENSE README.md CHANGELOG.md; do
  [ -f "$f" ] && install -m 0644 "$f" "$PREFIX/$f"
done

# ---- PATH -------------------------------------------------------------------
BIN="$PREFIX/bin"
case ":$PATH:" in
  *":$BIN:"*) echo "==> $BIN already on PATH" ;;
  *)
    # macOS defaults to zsh; fall back to bash_profile when that is the shell.
    PROFILE="$HOME/.zshrc"
    if [ -n "${BASH_VERSION:-}" ] && [ "${SHELL:-}" = "/bin/bash" ]; then
      PROFILE="$HOME/.bash_profile"
    fi
    LINE="export PATH=\"$BIN:\$PATH\"  # added by Aoxn install-macos.sh"
    if [ -f "$PROFILE" ] && grep -qF "$BIN" "$PROFILE"; then
      echo "==> $PROFILE already references $BIN"
    else
      printf '\n%s\n' "$LINE" >> "$PROFILE"
      echo "==> added $BIN to PATH in $PROFILE"
    fi
    export PATH="$BIN:$PATH"
    ;;
esac

# ---- self-test --------------------------------------------------------------
echo "==> aoxn doctor"
AOXN_HOME="$PREFIX" "$BIN/aoxn" doctor || {
  echo "warning: aoxn doctor reported a problem (see above)." >&2
}

echo ""
echo "==> installed Aoxn v${VERSION} to $PREFIX"
echo "    open a new shell (or: source ~/.zshrc), then: aoxn run examples/hello.ax"