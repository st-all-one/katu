#!/usr/bin/env bash
# Package katu for release — creates tar.gz/zip + sha256sums.txt
#
# Usage:
#   ./scripts/package.sh [VERSION]
#
# If VERSION is not provided, uses `git describe --tags --abbrev=0` or falls back to "dev".
# The script builds the release binary, packages it for the current target, and generates
# sha256sums.txt for all created assets.
#
# Output: target/release/katu-<version>-<target>.{tar.gz,zip} + sha256sums.txt

set -euo pipefail

# ── helpers ──────────────────────────────────────────────────────────────────

info() { printf '\033[34m==>\033[0m %s\n' "$*"; }
ok()   { printf '\033[32m  ✓\033[0m %s\n' "$*"; }
warn() { printf '\033[33m  !\033[0m %s\n' "$*" >&2; }
err()  { printf '\033[31m  ✗\033[0m %s\n' "$*" >&2; exit 1; }

# ── arguments ────────────────────────────────────────────────────────────────

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    VERSION="$(git describe --tags --abbrev=0 2>/dev/null || echo "")"
fi
if [ -z "$VERSION" ]; then
    VERSION="dev"
    warn "No git tag found; using version 'dev'"
fi

# Remove leading 'v' if present for the asset name
VERSION_NO_V="${VERSION#v}"

# ── detect target ────────────────────────────────────────────────────────────

detect_target() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)  os="linux" ;;
        Darwin) os="darwin" ;;
        MINGW*|MSYS*|CYGWIN*) os="windows" ;;
        *) err "Unsupported OS: $os" ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *) err "Unsupported architecture: $arch" ;;
    esac

    case "$os-$arch" in
        linux-x86_64)   echo "x86_64-unknown-linux-musl" ;;
        linux-aarch64)  echo "aarch64-unknown-linux-musl" ;;
        darwin-aarch64) echo "aarch64-apple-darwin" ;;
        darwin-x86_64)  echo "x86_64-apple-darwin" ;;
        windows-x86_64) echo "x86_64-pc-windows-msvc" ;;
        windows-aarch64) echo "aarch64-pc-windows-msvc" ;;
        *) err "No prebuilt binary for $os/$arch" ;;
    esac
}

TARGET="$(detect_target)"
info "Packaging katu ${VERSION_NO_V} for ${TARGET}"

# ── build ────────────────────────────────────────────────────────────────────

BUILD_DIR="target/release"
PACKAGE_DIR="${BUILD_DIR}/package"
ASSET="katu-${VERSION_NO_V}-${TARGET}"

info "Building release binary..."
cargo build --release -p katu --locked

# ── package ──────────────────────────────────────────────────────────────────

PACKAGE_DIR="$(pwd)/${BUILD_DIR}/package"
mkdir -p "${PACKAGE_DIR}"

if [ "$TARGET" = *-pc-windows-* ]; then
    # Windows: zip
    ASSET_EXT="zip"
    info "Creating ${ASSET}.zip..."
    (cd "${BUILD_DIR}" && zip -q -r "${PACKAGE_DIR}/${ASSET}.zip" katu.exe)
else
    # Unix: tar.gz
    ASSET_EXT="tar.gz"
    info "Creating ${ASSET}.tar.gz..."
    (cd "${BUILD_DIR}" && tar -czf "${PACKAGE_DIR}/${ASSET}.tar.gz" katu)
fi

# ── checksums ────────────────────────────────────────────────────────────────

info "Generating sha256sums.txt..."
(
    cd "${PACKAGE_DIR}"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "${ASSET}.${ASSET_EXT}" > sha256sums.txt
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "${ASSET}.${ASSET_EXT}" > sha256sums.txt
    else
        err "No SHA-256 tool found (install sha256sum or shasum)"
    fi
)

ok "Package created: ${PACKAGE_DIR}/${ASSET}.${ASSET_EXT}"
ok "Checksums: ${PACKAGE_DIR}/sha256sums.txt"
echo ""
info "Upload ${PACKAGE_DIR}/${ASSET}.${ASSET_EXT} and ${PACKAGE_DIR}/sha256sums.txt to the GitHub release"
