#!/usr/bin/env bash
# Demonstrates gRPC server reflection against the multiservice example
# using `buf curl` as an independent, schema-free client.
#
# Every RPC below is resolved through the server's reflection service —
# no proto files or descriptor sets are passed to buf curl.
#
# The server supports two descriptor sources (see build_reflector() in
# src/bin/server.rs); exercise both:
#   ./reflection-demo.sh                          # checked-in FileDescriptorSet
#   REFLECTION_SOURCE=pool ./reflection-demo.sh   # generated descriptor_pool()
#
# Requires: buf >= 1.30 (https://buf.build/docs/installation)
set -euo pipefail

SERVER_PID=""
ADDR="${ADDR:-127.0.0.1:8080}"
REFLECTION_SOURCE="${REFLECTION_SOURCE:-fds}"
echo "Reflection source: $REFLECTION_SOURCE"
BASE="http://$ADDR"
# gRPC needs HTTP/2; the example server speaks h2c via prior knowledge.
BUF_CURL=(buf curl --protocol grpc --http2-prior-knowledge)

if ! command -v buf >/dev/null 2>&1; then
    echo "error: buf is not installed - see https://buf.build/docs/installation" >&2
    exit 1
fi

cleanup() {
    if [ -n "$SERVER_PID" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

echo "Building multiservice example..."
cargo build -p multiservice-example --bin multiservice-server

echo "Starting multiservice-server on $ADDR..."
ADDR="$ADDR" REFLECTION_SOURCE="$REFLECTION_SOURCE" cargo run -p multiservice-example --bin multiservice-server &
SERVER_PID=$!

for _ in $(seq 1 30); do
    if curl -s "$BASE/health" > /dev/null 2>&1; then
        break
    fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "Server process died unexpectedly." >&2
        exit 1
    fi
    sleep 0.1
done

echo
echo "==> 1. List the server's services via the reflection RPC itself"
"${BUF_CURL[@]}" -d '{"listServices": ""}' \
    "$BASE/grpc.reflection.v1.ServerReflection/ServerReflectionInfo"

echo
echo "==> 2. Fetch the file that declares GreetService (descriptor bytes elided)"
"${BUF_CURL[@]}" -d '{"fileContainingSymbol": "anthropic.connectrpc.greet.v1.GreetService"}' \
    "$BASE/grpc.reflection.v1.ServerReflection/ServerReflectionInfo" \
    | sed 's/"\([A-Za-z0-9+\/]\{60\}\)[A-Za-z0-9+\/=]*"/"\1..."/'

echo
echo "==> 3. Call GreetService/Greet with the schema resolved via reflection"
"${BUF_CURL[@]}" -d '{"name": "reflection"}' \
    "$BASE/anthropic.connectrpc.greet.v1.GreetService/Greet"

echo
echo "==> 4. Call MathService/Add the same way"
"${BUF_CURL[@]}" -d '{"a": 19, "b": 23}' \
    "$BASE/anthropic.connectrpc.math.v1.MathService/Add"

echo
echo "==> 5. Same call, resolving the schema over the legacy v1alpha protocol"
"${BUF_CURL[@]}" --reflect-protocol grpc-v1alpha -d '{"name": "v1alpha"}' \
    "$BASE/anthropic.connectrpc.greet.v1.GreetService/Greet"

echo
echo "==> 6. Unknown symbols answer in-band with NOT_FOUND (error_code 5)"
"${BUF_CURL[@]}" -d '{"fileContainingSymbol": "no.such.Service"}' \
    "$BASE/grpc.reflection.v1.ServerReflection/ServerReflectionInfo"

echo
echo "All reflection demo calls succeeded."
