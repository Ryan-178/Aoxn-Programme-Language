#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT="$ROOT/dist/release"
SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --output) [ $# -ge 2 ] && [ -n "$2" ] || exit 2; OUTPUT="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h|--help) printf '%s\n' 'Usage: bash dist/package-linux.sh [--output DIR] [--skip-build]'; exit 0 ;;
    *) printf 'unknown option: %s\n' "$1" >&2; exit 2 ;;
  esac
done
[ "$(uname -s)" = Linux ] || { printf '%s\n' 'Linux packaging requires a native Linux host' >&2; exit 1; }
ARCH="$(uname -m)"
case "$ARCH" in x86_64|aarch64) ;; *) printf 'unsupported architecture: %s\n' "$ARCH" >&2; exit 1 ;; esac
cd "$ROOT"
if [ "$SKIP_BUILD" -eq 0 ]; then
  cargo build --release --locked --bin aoxn
fi
[ -x target/release/aoxn ] || { printf '%s\n' 'missing target/release/aoxn' >&2; exit 1; }
VERSION="$(target/release/aoxn version)"
VERSION="${VERSION#aoxn }"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][A-Za-z0-9.-]+)?$ ]] || exit 1
mkdir -p -- "$OUTPUT"
OUTPUT="$(realpath -- "$OUTPUT")"
NAME="Aoxn-$VERSION-linux-$ARCH"
[ ! -e "$OUTPUT/$NAME.tar.gz" ] && [ ! -e "$OUTPUT/$NAME.tar.gz.sha256" ] || {
  printf '%s\n' 'output already exists; choose an empty output directory' >&2; exit 1;
}
STAGE="$(mktemp -d)"
trap 'rm -rf -- "$STAGE"' EXIT
PACKAGE="$STAGE/$NAME"
mkdir -p "$PACKAGE/bin" "$PACKAGE/lib" "$PACKAGE/dist" "$PACKAGE/docs"
install -m 0755 target/release/aoxn "$PACKAGE/bin/aoxn"
install -m 0755 dist/install-linux.sh "$PACKAGE/dist/install-linux.sh"
cp -R stdlib "$PACKAGE/lib/stdlib"
cp -R examples "$PACKAGE/examples"
install -m 0644 docs/install.md "$PACKAGE/docs/install.md"
install -m 0644 LICENSE README.md SECURITY.md CHANGELOG.md "$PACKAGE/"
tar -C "$STAGE" -czf "$OUTPUT/$NAME.tar.gz" "$NAME"
(cd "$OUTPUT" && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")
printf 'Packaged %s/%s.tar.gz (not published)\n' "$OUTPUT" "$NAME"
