# Bazel example

End-to-end demonstration that the buffa and connectrpc protoc plugins
work cleanly under Bazel. A `bazel test //...` invocation:

1. Runs `protoc` with all three plugins (`protoc-gen-buffa`,
   `protoc-gen-buffa-packaging`, `protoc-gen-connect-rust`) via a
   `genrule`, producing four `.rs` files. A second `genrule` runs the
   same plugins through `buf generate` (driven by the `rules_buf`
   toolchain) and a `sh_test` proves the two output trees are
   byte-identical.
2. Pulls `buffa`, `connectrpc`, and their transitive deps from crates.io
   via `rules_rust` + `crates_universe`.
3. Compiles a `rust_library` whose srcs include `src/lib.rs` plus the
   four generated files.
4. Runs three `rust_test`s that exercise the generated message types
   (construction, encode/decode round-trip via `buffa::Message`) and
   the connectrpc service-stub constants.
5. Runs `buf_lint_test` (from `rules_buf`) over the proto sources,
   enforcing the buf STANDARD lint ruleset on every CI run.

No `env!`-baked paths anywhere in the generated code, no checked-in
generated `.rs` files, no `build.rs`.

## Layout

| File | Purpose |
| --- | --- |
| `MODULE.bazel` | bzlmod deps: `protobuf`, `rules_proto`, `rules_buf`, `rules_rust`, `crates_universe`. |
| `BUILD.bazel` | The two codegen genrules (`gen_code`, `gen_code_via_buf`), the `gen_outputs_match_test` diff check, the `greet_proto` `proto_library`, `greet_proto_lint`, the `greet_lib` library, and `greet_lib_test`. |
| `.bazelrc` | Enables `--experimental_proto_descriptor_sets_include_source_info` so `buf_lint_test` can read source positions from descriptor sets. |
| `buf.yaml` | The buf module + lint/breaking config, shared with the local `buf` CLI. |
| `Cargo.toml` / `Cargo.lock` | Dependency manifest fed to `crates_universe.from_cargo`. |
| `stub.rs` | Empty crate body — `cargo metadata` needs a target so `crates_universe` can resolve the graph. |
| `proto/anthropic/connectrpc/examples/greet/v1/greet.proto` | Minimal `GreetService` definition. |
| `tools/BUILD.bazel` | Exposes the plugin binaries as Bazel labels. |
| `setup.sh` | Builds the plugins via cargo and symlinks them under `tools/`. |
| `tools/diff_outputs.sh` | Pairwise byte-diff invoked by `gen_outputs_match_test`. |
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
bazel test //...
```

On the first invocation Bazel will build `protoc` from source (~30s),
fetch the `buf` toolchain, fetch + compile ~70 crates from crates.io,
and produce the generated sources. Subsequent runs are cached. Expected
output:

```
//:gen_outputs_match_test  PASSED
//:greet_lib_test          PASSED
//:greet_proto_lint        PASSED
```

The `greet_lib_test` log shows three Rust unit tests passing:

```
test tests::message_types_are_constructible ... ok
test tests::message_types_round_trip_through_buffa ... ok
test tests::service_name_constant_is_correct ... ok
```

## How the codegen rules work

There are two parallel codegen genrules. Both drive the same three
plugins and emit byte-identical output (verified by
`gen_outputs_match_test`):

| Rule | Tool | Plugin config |
| --- | --- | --- |
| `gen_code` | `protoc` directly | inline in the genrule's `cmd` |
| `gen_code_via_buf` | `buf generate` (from the `rules_buf` toolchain) | a `buf.gen.yaml` generated at build time, with plugin paths resolved via `$(location ...)` |

Output naming is deterministic from the proto file path: a file at
`proto/anthropic/connectrpc/examples/greet/v1/greet.proto` becomes
`anthropic.connectrpc.examples.greet.v1.greet.rs`. The packaging plugin
emits a `mod.rs` that nests `pub mod` blocks matching the proto's
`package` declaration and `include!`s the per-file output as a sibling.

Two output trees per genrule — one with buffa message types, one with
connectrpc service stubs — because the plugins emit colliding filenames
(both `mod.rs` and `<package>.rs`). The packaging plugin runs twice,
once over each tree (the second invocation passes `filter=services` so
files without services are skipped from the connect output).

The generated `mod.rs` files use sibling-relative `include!("foo.rs")`,
so consuming the output requires no `env!("OUT_DIR")` indirection.
`src/lib.rs` mounts each tree with a single `#[path = "..."]` attribute.

### When to choose which

For this single-proto example the two patterns are equivalent. In a
real codebase:

- **`protoc` directly** is cleanest when plugin invocations live only in
  Bazel — fewer files, every flag visible in `BUILD.bazel`, no
  dependence on the buf toolchain.
- **`buf generate`** wins when you want the same plugin config to drive
  both `bazel build` and the local `buf generate` workflow. Plugin
  options live in one shared `buf.gen.yaml` instead of being duplicated
  between Bazel and developer scripts.

## Why a separate `Cargo.toml` for the example?

`crates_universe.from_cargo` reads a Cargo manifest to discover the
dependency graph, so the example needs one. It is a stub crate (the
`[lib]` points at an empty `stub.rs`) — Bazel does the actual building.

## Why two passes of `protoc-gen-buffa-packaging`?

The packaging plugin emits the `mod.rs` module-tree file. It runs once
for each output tree we want a `mod.rs` for: once over the buffa output
(default behavior, includes every file) and once over the connect output
with `filter=services` (skip files that contain no services).

## What `rules_buf` adds (and doesn't)

`rules_buf` provides `buf_dependencies`, `buf_lint_test`,
`buf_breaking_test`, and `buf_format` — proto source management and
quality checks that work alongside `rules_proto`'s `proto_library`. It
does **not** provide a `buf_generate` rule, so when you want
buf-driven codegen as a Bazel build action you wrap the `buf` binary
(supplied by the `rules_buf_toolchains` target) in a `genrule`
yourself, as `gen_code_via_buf` does.

Here we wire in `buf_lint_test` against the `greet_proto`
`proto_library`, sharing config (`buf.yaml`) with the local `buf` CLI.
`buf_breaking_test` against a snapshot or git ref would be the natural
next addition. If the proto imported from BSR (e.g.
`buf.build/protocolbuffers/wellknowntypes`), `buf_dependencies` would
fetch and expose those modules — this example has no BSR imports.
