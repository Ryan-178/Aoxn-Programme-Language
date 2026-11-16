#!/usr/bin/env bash
# Native Linux source installer. No toolchains are downloaded by this script.
set -euo pipefail

usage() {
  printf '%s\n' 'Usage: bash dist/install-linux.sh [options]' \
    '  --prefix PATH      Absolute install root (default: ~/.local/aoxn)' \
    '  --skip-build       Reuse target/release/aoxn; cargo is not required' \
    '  --modify-path      Append a safely quoted PATH entry to ~/.profile' \
    '  --no-modify-path   Leave shell profiles untouched (default)' \
    '  -h, --help         Show this help on any platform' \
    'Requires clang and, unless --skip-build, cargo. Downloads nothing itself.'
}
fail() { printf 'install-linux.sh: %s\n' "$*" >&2; exit 1; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="${HOME:?HOME must be set}/.local/aoxn"
SKIP_BUILD=0
MODIFY_PATH=0
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix)
      [ $# -ge 2 ] && [[ "$2" != --* ]] || fail '--prefix requires an absolute path'
      PREFIX="$2"; shift 2 ;;
    --prefix=*) PREFIX="${1#--prefix=}"; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --modify-path) MODIFY_PATH=1; shift ;;
    --no-modify-path) MODIFY_PATH=0; shift ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown option: $1" ;;
  esac
done

[ "$(uname -s)" = Linux ] || fail 'this installer requires Linux'
# Validate before building or writing anything. Resolve aliases and symlinked
# parents too, so /tmp/.. or a symlink to / cannot bypass the root guard.
[[ "$PREFIX" = /* ]] || fail '--prefix must be a nonempty absolute path'
[[ "$PREFIX" != *$'\n'* && "$PREFIX" != *$'\r'* && "$PREFIX" != *:* ]] || \
  fail '--prefix must not contain newlines or PATH separators'
PREFIX="$(realpath -m -- "$PREFIX")" || fail 'cannot resolve --prefix'
case "$PREFIX" in
  /|/bin|/sbin|/usr|/usr/bin|/usr/sbin|/usr/lib|/usr/local|/lib|/lib64|/etc|/var|/opt|/home|/root|/tmp|/dev|/proc|/sys|/boot|/run|/mnt|/media)
    fail 'unsafe --prefix: choose a dedicated Aoxn directory' ;;
esac
[ "$PREFIX" != "$(realpath -m -- "$HOME")" ] || fail '--prefix must not be HOME itself'
ROOT="$(realpath -m -- "$ROOT")"
case "$PREFIX/" in
  "$ROOT/"*) fail '--prefix must be outside the source or package tree' ;;
esac
case "$ROOT/" in
  "$PREFIX/"*) fail '--prefix must not contain the source or package tree' ;;
esac
CLANG="${AOXN_CLANG:-clang}"
command -v "$CLANG" >/dev/null 2>&1 || fail 'clang not found; set AOXN_CLANG or install clang with your system package manager'
"$CLANG" --version >/dev/null 2>&1 || fail 'selected clang cannot run'

cd "$ROOT"
if [ -x "$ROOT/bin/aoxn" ] && [ -d "$ROOT/lib/stdlib" ]; then
  COMPILER="$ROOT/bin/aoxn"
  STDLIB="$ROOT/lib/stdlib"
else
  if [ "$SKIP_BUILD" -eq 0 ]; then
    command -v cargo >/dev/null 2>&1 || fail 'cargo not found; install Rust first or use --skip-build'
    cargo build --release --locked --bin aoxn
  fi
  COMPILER="$ROOT/target/release/aoxn"
  STDLIB="$ROOT/stdlib"
fi
[ -f "$COMPILER" ] && [ -x "$COMPILER" ] || fail 'missing executable compiler; run without --skip-build'
[ "$PREFIX" != "$ROOT" ] || fail '--prefix must not be the package root'

printf '==> installing to %s\n' "$PREFIX"
mkdir -p -- "$PREFIX/bin" "$PREFIX/lib/stdlib" "$PREFIX/examples" "$PREFIX/docs"
install -m 0755 -- "$COMPILER" "$PREFIX/bin/aoxn"
# Merge entire trees, including nested modules and Rust bridge sources. Never
# delete prefix contents: existing unrelated files survive a reinstall.
cp -R -- "$STDLIB/." "$PREFIX/lib/stdlib/"
cp -R -- "$ROOT/examples/." "$PREFIX/examples/"
if [ -f "$ROOT/docs/install.md" ]; then
  install -m 0644 -- "$ROOT/docs/install.md" "$PREFIX/docs/install.md"
fi
for f in LICENSE README.md CHANGELOG.md; do
  if [ -f "$ROOT/$f" ]; then
    install -m 0644 -- "$ROOT/$f" "$PREFIX/$f"
  fi
done

BIN="$PREFIX/bin"
printf '%s\n' '==> aoxn doctor'
# A broken installation is a failure, not a successful install with a warning.
AOXN_HOME="$PREFIX" AOXN_STDLIB="$PREFIX/lib/stdlib" PATH="$BIN:$PATH" "$BIN/aoxn" doctor || fail 'aoxn doctor failed'

if [ "$MODIFY_PATH" -eq 1 ]; then
  # POSIX single-quote escaping, safe even for spaces, $, ` and literal quotes.
  QUOTED_BIN="${BIN//\'/\'\\\'\'}"
  LINE="export PATH='$QUOTED_BIN':\"\$PATH\" # added by Aoxn install-linux.sh"
  PROFILE="$HOME/.profile"
  if [ ! -f "$PROFILE" ] || ! grep -qxF -- "$LINE" "$PROFILE"; then
    printf '\n%s\n' "$LINE" >> "$PROFILE"
  fi
  printf '==> PATH entry saved in %s\n' "$PROFILE"
fi
printf '==> installed Aoxn to %s\n' "$PREFIX"
printf '    Compiler: %s/aoxn (add this bin directory to PATH if needed)\n' "$BIN"
