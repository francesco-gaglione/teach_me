#!/bin/sh
# Downloads the pinned PDFium build (bblanchon/pdfium-binaries) into DEST and
# checks its sha256. Single source of truth for the Docker image and for CI.
# Usage: scripts/fetch-pdfium.sh DEST [amd64|arm64]   (arch defaults to this machine's)
# Result: DEST/lib/libpdfium.so (set TEACH_ME_PDFIUM_DIR=DEST/lib), DEST/LICENSE, DEST/licenses.
# To update: pick a release, copy the sha256 of each tarball from its page, and
# run the PDF tests against it.
set -eu

PDFIUM_RELEASE=8138
PDFIUM_SHA256_AMD64=d6b1e28950a0392590d79bbdbb7eb19686872b688f8a36f170acb19e41473113
PDFIUM_SHA256_ARM64=c70b1ce4d13257f741af19e0aeee575890875995eb81370a65be942e601a1862

dest="${1:?usage: fetch-pdfium.sh DEST [amd64|arm64]}"
target="${2:-}"
if [ -z "$target" ]; then
    case "$(uname -m)" in
        x86_64) target=amd64 ;;
        aarch64 | arm64) target=arm64 ;;
        *) echo "no PDFium build for $(uname -m)" >&2; exit 1 ;;
    esac
fi
case "$target" in
    amd64) arch=x64; sha="$PDFIUM_SHA256_AMD64" ;;
    arm64) arch=arm64; sha="$PDFIUM_SHA256_ARM64" ;;
    *) echo "no PDFium build for $target" >&2; exit 1 ;;
esac

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
# Retries cover transient network failures; the hash check below still fails closed.
curl -fsSL --retry 3 --retry-all-errors -o "$tmp" \
    "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${PDFIUM_RELEASE}/pdfium-linux-${arch}.tgz"
echo "${sha}  $tmp" | sha256sum -c -
mkdir -p "$dest"
tar -xzf "$tmp" -C "$dest" lib/libpdfium.so LICENSE licenses
