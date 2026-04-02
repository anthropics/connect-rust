#!/usr/bin/env bash
# Diffs the protoc-direct codegen outputs against the buf-driven outputs.
# Both genrules pass the same plugins identical inputs, so the .rs files
# must be byte-identical.
set -euo pipefail

if (( $# == 0 || $# % 2 != 0 )); then
    echo "usage: $0 <a1> <b1> [<a2> <b2> ...]" >&2
    exit 2
fi

while (( $# > 0 )); do
    a="$1"; b="$2"; shift 2
    if ! diff -u "$a" "$b"; then
        echo "FAIL: $a differs from $b" >&2
        exit 1
    fi
done

echo "All output pairs match."
