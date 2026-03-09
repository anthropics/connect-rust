#!/usr/bin/env bash
#
# Download the ConnectRPC conformance test runner.
#
# Usage: ./download-conformance.sh [VERSION]
#
# If VERSION is not specified, defaults to the version in Taskfile.yaml.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
BIN_DIR="${ROOT_DIR}/bin"
CONFORMANCE_BIN="${BIN_DIR}/connectconformance"

# Default version - can be overridden by argument
VERSION="${1:-v1.0.5}"

# Detect OS
case "$(uname -s)" in
    Linux)  OS="Linux" ;;
    Darwin) OS="Darwin" ;;
    *)      OS="$(uname -s)" ;;
esac

# Detect architecture
case "$(uname -m)" in
    x86_64|amd64)   ARCH="x86_64" ;;
    aarch64|arm64)  ARCH="arm64" ;;
    *)              ARCH="$(uname -m)" ;;
esac

TARBALL="connectconformance-${VERSION}-${OS}-${ARCH}.tar.gz"
DOWNLOAD_URL="https://github.com/connectrpc/conformance/releases/download/${VERSION}/${TARBALL}"

# Check if already downloaded
if [[ -f "$CONFORMANCE_BIN" ]]; then
    echo "Conformance runner already exists at ${CONFORMANCE_BIN}"
    echo "Delete it first if you want to re-download."
    exit 0
fi

mkdir -p "$BIN_DIR"
cd "$BIN_DIR"

echo "Downloading connectconformance ${VERSION} for ${OS}-${ARCH}..."

# Try gh CLI first (handles authentication for private repos, rate limits)
if command -v gh &> /dev/null; then
    # Check if gh is authenticated
    if gh auth status &> /dev/null; then
        echo "Using gh CLI..."
        if gh release download "${VERSION}" \
            --repo connectrpc/conformance \
            --pattern "${TARBALL}" \
            --dir . 2>/dev/null; then
            tar xzf "${TARBALL}"
            rm -f "${TARBALL}"
            echo "Downloaded to ${CONFORMANCE_BIN}"
            exit 0
        else
            echo "gh download failed, falling back to curl..."
        fi
    else
        echo "gh CLI not authenticated, using curl instead..."
    fi
fi

# Fall back to curl
echo "Downloading from ${DOWNLOAD_URL}..."
if curl -fsSL -o "${TARBALL}" "${DOWNLOAD_URL}"; then
    tar xzf "${TARBALL}"
    rm -f "${TARBALL}"
    echo "Downloaded to ${CONFORMANCE_BIN}"
else
    echo "Error: Failed to download ${TARBALL}"
    echo "URL: ${DOWNLOAD_URL}"
    echo ""
    echo "Please check:"
    echo "  - The version ${VERSION} exists"
    echo "  - Your platform ${OS}-${ARCH} is supported"
    echo "  - You have network connectivity"
    exit 1
fi
