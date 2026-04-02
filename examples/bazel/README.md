# Bazel example

End-to-end demonstration that the buffa and connectrpc protoc plugins
work cleanly under Bazel. A `bazel test //:greet_lib_test` invocation:

1. Runs `protoc` with all three plugins (`protoc-gen-buffa`,
   `protoc-gen-buffa-packaging`, `protoc-gen-connect-rust`) via a
   `genrule`, producing four `.rs` files.
2. Pulls `buffa`, `connectrpc`, and their transitive deps from crates.io
   via `rules_rust` + `crates_universe`.
3. Compiles a `rust_library` whose srcs include `src/lib.rs` plus the
   four generated files.
4. Runs three `rust_test`s that exercise the generated message types
   (construction, encode/decode round-trip via `buffa::Message`) and
   the connectrpc service-stub constants.

No `env!`-baked paths anywhere in the generated code, no checked-in
generated `.rs` files, no `build.rs`.

## Layout

| File | Purpose |
| --- | --- |
| `MODULE.bazel` | bzlmod deps: `protobuf`, `rules_rust`, `crates_universe`. |
| `BUILD.bazel` | The `gen_code` genrule, the `greet_lib` library, and `greet_lib_test`. |
| `Cargo.toml` / `Cargo.lock` | Dependency manifest fed to `crates_universe.from_cargo`. |
| `stub.rs` | Empty crate body — `cargo metadata` needs a target so `crates_universe` can resolve the graph. |
| `proto/greet/v1/greet.proto` | Minimal `GreetService` definition. |
| `tools/BUILD.bazel` | Exposes the plugin binaries as Bazel labels. |
| `setup.sh` | Builds the plugins via cargo and symlinks them under `tools/`. |
| `src/lib.rs` | Mounts both generated trees via `#[path = "..."]` and contains the tests. |

## Setup

Build the plugin binaries (one-time, repeat if you change the codegen
crates):

```sh
./setup.sh
```

This builds `protoc-gen-buffa` and `protoc-gen-buffa-packaging` from the
sibling `buffa` checkout, builds `protoc-gen-connect-rust` from this
repo, and symlinks all three into `tools/`. The symlinks are gitignored.

## Run the build

```sh
bazel test //:greet_lib_test
```

On the first invocation Bazel will build `protoc` from source (~30s),
fetch + compile ~70 crates from crates.io, and produce the generated
sources. Subsequent runs are cached. Expected output:

```
running 3 tests
test tests::message_types_are_constructible ... ok
test tests::message_types_round_trip_through_buffa ... ok
test tests::service_name_constant_is_correct ... ok
```

## How the codegen rule works

The `gen_code` genrule invokes `protoc` once with all three plugins
attached. Output naming is deterministic from the proto file path: a
file at `proto/greet/v1/greet.proto` becomes `greet.v1.greet.rs`. The
packaging plugin emits a `mod.rs` that nests `pub mod` blocks matching
the proto's `package` declaration and `include!`s the per-file output
as a sibling.

Two output trees are produced — one with buffa message types, one with
connectrpc service stubs — because the plugins emit colliding filenames
(both `mod.rs` and `<proto-path>.rs`). The packaging plugin runs twice,
once over each tree (the second invocation passes `filter=services` so
files without services are skipped from the connect output).

The generated `mod.rs` files use sibling-relative `include!("foo.rs")`,
so consuming the output requires no `env!("OUT_DIR")` indirection.
`src/lib.rs` mounts each tree with a single `#[path = "..."]` attribute.

## Why a separate `Cargo.toml` for the example?

`crates_universe.from_cargo` reads a Cargo manifest to discover the
dependency graph, so the example needs one. It is a stub crate (the
`[lib]` points at an empty `stub.rs`) — Bazel does the actual building.

## Why two passes of `protoc-gen-buffa-packaging`?

The packaging plugin emits the `mod.rs` module-tree file. It runs once
for each output tree we want a `mod.rs` for: once over the buffa output
(default behavior, includes every file) and once over the connect output
with `filter=services` (skip files that contain no services).

## Why not use `rules_buf` for codegen?

`rules_buf` provides `buf_dependencies`, `buf_lint_test`, and
`buf_breaking_test` — proto source management and quality checks. It
does **not** provide a code generation rule. Codegen still goes through
either a custom Skylark rule or, as here, a `genrule` invoking `protoc`.

A future iteration could add `buf_lint_test` and `buf_breaking_test`
targets alongside the codegen genrule to demonstrate the full
`rules_buf` integration story.
