#!/usr/bin/env bash
# Builds the three protoc plugins via cargo and symlinks them under tools/
# so the Bazel build can reference them as labels.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONNECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUFFA_ROOT="$(cd "$CONNECT_ROOT/../buffa" && pwd)"

if [[ ! -d "$BUFFA_ROOT" ]]; then
    echo "expected buffa checkout at $BUFFA_ROOT" >&2
    exit 1
fi

echo "Building protoc-gen-buffa and protoc-gen-buffa-packaging..."
(cd "$BUFFA_ROOT" && cargo build --release \
    -p protoc-gen-buffa \
    -p protoc-gen-buffa-packaging)

echo "Building protoc-gen-connect-rust..."
(cd "$CONNECT_ROOT" && cargo build --release -p connectrpc-codegen --bin protoc-gen-connect-rust)

echo "Linking plugin binaries into tools/..."
ln -sfn "$BUFFA_ROOT/target/release/protoc-gen-buffa" "$SCRIPT_DIR/tools/protoc-gen-buffa"
ln -sfn "$BUFFA_ROOT/target/release/protoc-gen-buffa-packaging" "$SCRIPT_DIR/tools/protoc-gen-buffa-packaging"
ln -sfn "$CONNECT_ROOT/target/release/protoc-gen-connect-rust" "$SCRIPT_DIR/tools/protoc-gen-connect-rust"

echo "Done. You can now run: bazel build //..."
