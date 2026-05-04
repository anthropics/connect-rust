# Changelog

All notable changes to connectrpc will be documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
with the [Rust 0.x convention](https://doc.rust-lang.org/cargo/reference/semver.html):
breaking changes increment the minor version (0.2 → 0.3), additive changes
increment the patch version.

## [Unreleased]

### Breaking

- **buffa 0.4**: adapted to buffa's per-package stitcher layout
  ([buffa#62]) and `ViewEncode` ([buffa#55]). Generated view types
  now live under `<pkg>::__buffa::view::FooView` (was `<pkg>::FooView`);
  oneof enums under `<pkg>::__buffa::oneof::<msg>::Kind` and
  `<pkg>::__buffa::view::oneof::<msg>::Kind`. Service stubs are
  appended to buffa's `<stem>.rs` content file in the unified path,
  and emit their own `<pkg>.mod.rs` stitcher in the split path.
  `buffa_types::Any.value` is now `bytes::Bytes` (was `Vec<u8>`).
  buffa's size cache is now externalized ([buffa#22]): generated
  structs no longer carry `__buffa_cached_size`, and
  `Message::compute_size`/`write_to` take `&mut SizeCache`. The
  provided `encode_to_bytes()` / `encoded_len()` are unchanged;
  connectrpc itself only uses those, but direct callers of
  `compute_size()` should switch to `encoded_len()`.
- **`connectrpc-codegen`**: `Options` now embeds the buffa
  `CodeGenConfig` directly as `Options::buffa` instead of mirroring
  individual fields ([#34]). The previous per-field shims
  (`strict_utf8_mapping`, `generate_json`, `extern_paths`,
  `emit_register_fn`) are gone; set `options.buffa.<field>` instead.
  `CodeGenConfig` is re-exported from `connectrpc_codegen::codegen` and
  `connectrpc_build`. `connectrpc_build::Config` keeps its existing
  builder methods as thin shims and gains `.buffa_config(cfg)` for
  wholesale replacement. `generate_views = true` is still enforced.
- **`ConnectError` shrunk from 248 to 72 bytes** ([#61]). The
  `response_headers` and `trailers` fields are now crate-private
  `Option<Box<http::HeaderMap>>` (was `pub http::HeaderMap`), so
  `Result<_, ConnectError>` no longer trips
  `clippy::result_large_err`. New accessors replace direct field
  access: `response_headers()` / `trailers()` (borrow, empty map if
  unset), `response_headers_mut()` / `trailers_mut()`, and
  `set_response_headers()` / `set_trailers()`. The `with_headers()` /
  `with_trailers()` builders keep their signatures. Behaviour notes:
  `with_headers` / `with_trailers` / `set_*` now normalize an empty
  `HeaderMap` to "unset" (observationally identical via the
  accessors), and the `Debug` output for an unset map now shows
  `None` instead of `{}`.
- **Handler signatures redesigned** ([#7]): the generated service
  trait no longer threads a single `Context` in and out. Handlers
  now receive a read-only `RequestContext` (headers, deadline,
  extensions) and return `ServiceResult<B>` =
  `Result<Response<B>, ConnectError>`, where `Response<B>` carries
  the body plus optional response headers/trailers/compression hint.
  Unary and client-stream methods return
  `ServiceResult<impl Encodable<Out>>`; server-stream and bidi
  return `ServiceResult<ServiceStream<Out>>`. `Response::ok(body)` is
  the bare-body happy-path shorthand; for streaming bodies use
  `Response::stream_ok(s)`. `Encodable<M>` is the new "encodes as
  M" bound on response bodies. The old `Context` type is removed.

  ```rust
  // before
  async fn say(&self, ctx: Context, req: ...) -> Result<(SayResponse, Context), ConnectError> {
      Ok((SayResponse { ... }, ctx))
  }
  // after
  async fn say(&self, _ctx: RequestContext, req: ...) -> ServiceResult<SayResponse> {
      Response::ok(SayResponse { ... })
  }
  ```
- **View response bodies** ([#7]): unary and client-stream trait
  methods are now `<'a>(&'a self, ...) -> ServiceResult<impl
  Encodable<Out> + use<'a, Self>>`, so a handler can return a body
  that borrows from `&self`. Codegen emits `impl Encodable<Out> for
  OutView<'_>` and for `OwnedView<OutView<'static>>` per RPC output
  type (proto via `ViewEncode`; JSON returns an `unimplemented`
  error since view types lack `Serialize`). The new
  `MaybeBorrowed<M, V>` enum lets a handler return either: see
  `benches/rpc/benches/filter_handler.rs` for a redaction example
  (~1.65x at the codec layer when no modification is needed).
  `ViewHandler`/`ViewClientStreamingHandler` now take `CodecFormat`
  and return the response already encoded, dropping the `Res` type
  param.

### Added

- **`connectrpc::include_generated!()`**: shorthand macro for
  `include!(concat!(env!("OUT_DIR"), "/_connectrpc.rs"))`, mirroring
  `tonic::include_proto!`. An optional filename argument supports
  projects that customise the output via `Config::include_file` ([#50]).
- **`connectrpc-build`**: `Config::emit_rerun_directives(bool)` to suppress
  the `cargo:rerun-if-changed=` lines when running outside a Cargo
  `build.rs` context (e.g. from a Bazel genrule or standalone host tool).
  Default remains `true`.

[#50]: https://github.com/anthropics/connect-rust/issues/50
[#7]: https://github.com/anthropics/connect-rust/issues/7
[#34]: https://github.com/anthropics/connect-rust/issues/34
[#61]: https://github.com/anthropics/connect-rust/issues/61
[buffa#22]: https://github.com/anthropics/buffa/pull/22
[buffa#55]: https://github.com/anthropics/buffa/pull/55
[buffa#62]: https://github.com/anthropics/buffa/pull/62

## [0.3.3] - 2026-04-17

### Fixed

- **`connectrpc-build` no longer emits invalid
  `cargo:rerun-if-changed` directives in `Precompiled` input mode**
  ([#56]). When a precompiled `FileDescriptorSet` was supplied instead
  of `.proto` source files, `.files()` paths were still being passed
  through to cargo, causing spurious rebuild triggers on paths that
  don't exist in that mode.

### Changed

- **MSRV is now declared as Rust 1.88** on the workspace and verified
  in CI ([#44]). The code has required 1.88 since v0.3.2 (let-chains);
  this commit documents the requirement in `Cargo.toml` and adds an
  explicit CI check.

### Added

- New `examples/streaming-tour` and `examples/middleware` crates,
  plus a user guide under `docs/guide.md` ([#46], [#48]).

[#44]: https://github.com/anthropics/connect-rust/pull/44
[#46]: https://github.com/anthropics/connect-rust/pull/46
[#48]: https://github.com/anthropics/connect-rust/pull/48
[#56]: https://github.com/anthropics/connect-rust/pull/56

## [0.3.2] - 2026-04-03

### Fixed

- **Generated service code now compiles when multiple services are
  `include!`d into the same Rust module** ([#32]). The codegen previously
  emitted top-level `use` statements that collided with E0252 when
  buffa-packaging's flat-output strategy concatenated several service
  files into one module. Bindings now use fully-qualified paths
  throughout (`::connectrpc::Context`, `::buffa::view::OwnedView`,
  `::http_body::Body`, etc.), so multiple service files can coexist in
  the same `mod` block.

### Changed

- **Generated client methods reference the per-service `*_SERVICE_NAME`
  const** ([#16]) instead of repeating the fully-qualified service name
  as a string literal at every call site. Matches the server-side
  router.
- **Workspace `tokio` feature footprint narrowed** ([#19]). The published
  `connectrpc` crate previously inherited the full workspace tokio
  feature set (`macros`, `net`, `signal`, `rt-multi-thread`, ...) when
  `workspace = true` was inlined at publish time. It now requests only
  `rt`, `io-util`, `sync`, `time`, plus `net` when the `client` or
  `server` feature is enabled. Downstream crates that use `tokio`
  directly should declare their own features rather than relying on
  transitive activation.
- **Workspace dependency updates** ([#37]).

### Added

- **`wasm32-unknown-unknown` target compatibility** ([#19]) for the
  `connectrpc` crate with default features off. A new
  `examples/wasm-client` demonstrates a Fetch-based `ClientTransport`
  implementation with browser-based integration tests via `wasm-pack`.
  Currently exercises unary calls without deadlines; timeouts and
  streaming require additional setup beyond the example.

[#16]: https://github.com/anthropics/connect-rust/pull/16
[#19]: https://github.com/anthropics/connect-rust/pull/19
[#32]: https://github.com/anthropics/connect-rust/pull/32
[#37]: https://github.com/anthropics/connect-rust/pull/37

## [0.3.1] - 2026-04-02

### Added

- **`emit_register_fn` option** ([#35]) on `connectrpc_codegen::codegen::Options`
  and `connectrpc_build::Config`, plumbing through to
  `buffa_codegen::CodeGenConfig::emit_register_fn`. Set to `false` to suppress
  the per-file `register_types(&mut TypeRegistry)` aggregator when multiple
  generated files are `include!`d into the same module (the identically-named
  functions would otherwise collide). The protoc plugin accepts a matching
  `no_register_fn` parameter for path-compat with the unified `connectrpc-build`
  flow.

[#35]: https://github.com/anthropics/connect-rust/pull/35

## [0.3.0] - 2026-04-02

### Changed

- **Upgraded `buffa` to 0.3.0** ([#24]). buffa 0.3 renames `AnyRegistry` to
  `TypeRegistry` (with `JsonAnyEntry` and `register_json_any()` replacing the
  old `AnyTypeEntry` / `register()`). Generated code and the runtime crate
  now use the new types; users who construct a registry manually for
  `google.protobuf.Any` JSON encoding will need to migrate.
- **`connectrpc-build` only rewrites output files when content changes**
  ([#22]). Preserves mtimes so touching one `.proto` no longer triggers a
  full downstream recompile of every generated `.rs` file. Mirrors
  prost-build's `write_file_if_changed`.

### Added

- **mTLS peer credentials and remote address are now available to handlers**
  ([#31]) via `Context::extensions`. The built-in server inserts `PeerAddr`
  (always) and `PeerCerts` (when `server-tls` is enabled and the client
  presented a certificate chain) into every request's extensions; handlers
  read them with `ctx.extensions.get::<PeerAddr>()` /
  `ctx.extensions.get::<PeerCerts>()`. Custom HTTP stacks (axum, raw hyper)
  can insert the same types from a tower layer so handler code stays
  transport-agnostic.
- **`Server::from_listener(TcpListener)`** ([#31]) wraps a pre-bound
  listener, allowing socket options (`IPV6_V6ONLY=false` for dual-stack,
  `SO_REUSEPORT`, inherited file descriptors) to be configured before
  handing the listener to connectrpc.
- **`Http2Connection::lazy_with_connector` / `connect_with_connector`** ([#15])
  as the generic transport escape hatch — supply any `tower::Service<Uri>`
  yielding a `hyper::rt::Read + Write` stream and the library runs the h2
  handshake over it. `lazy_unix` / `connect_unix` are thin wrappers for
  Unix domain sockets.
- **Codegen now rejects RPC method names that collide after `to_snake_case`**
  ([#28]). `rpc GetFoo(...)` and `rpc get_foo(...)` in the same service
  previously emitted duplicate `fn get_foo` and failed with a rustc error
  pointing at generated code; the build script now fails with a clear error
  naming both proto methods. Also catches a method whose name collides with
  another's `_with_options` client variant.

### Fixed

- **RPC methods whose snake_case names are Rust keywords now generate valid
  code** ([#23], [#26]). `rpc Move(...)` previously emitted `fn move(...)`
  and failed at build-script time. Method idents are now routed through
  buffa's keyword escaper, producing `r#move` (or a `_` suffix for the four
  keywords that cannot be raw identifiers).
- **`service Self {}` no longer generates `trait Self`** ([#27]). The handler
  trait is suffixed to `Self_`; the `SelfExt` / `SelfClient` / `SelfServer`
  derivatives are unaffected since the suffix already de-keywords them.

[#15]: https://github.com/anthropics/connect-rust/pull/15
[#22]: https://github.com/anthropics/connect-rust/pull/22
[#23]: https://github.com/anthropics/connect-rust/issues/23
[#24]: https://github.com/anthropics/connect-rust/pull/24
[#26]: https://github.com/anthropics/connect-rust/pull/26
[#27]: https://github.com/anthropics/connect-rust/pull/27
[#28]: https://github.com/anthropics/connect-rust/pull/28
[#31]: https://github.com/anthropics/connect-rust/pull/31

## [0.2.1] - 2026-03-18

### Fixed

- **`BidiStream` half-duplex deadlock on `SharedHttp2Connection`** ([#2], [#4]).
  `call_bidi_stream` stored the transport's `send()` future unpolled, so for
  transports where that future contains the connect/handshake/stream work
  (i.e. not hyper's pooled client), the HTTP request never initiated until
  the first `message()` call. The half-duplex pattern (send all, close,
  then read) would buffer into the 32-deep `ChannelBody` mpsc with nobody
  draining it and deadlock on the 33rd send. The send future is now
  spawned so the request streams immediately.
- **TLS connections to IPv6 literal URIs failed** ([#1], [#3]). `Uri::host()`
  returns `[::1]` with brackets, which `rustls_pki_types::ServerName`
  rejected as an invalid DNS name. Brackets are now stripped so the
  address parses as `ServerName::IpAddress`.
- **README required-dependencies example showed `buffa = "0.1"`** instead
  of `"0.2"`. The `connectrpc` crate bakes the workspace README via
  `readme = "../README.md"`, so the crates.io page for 0.2.0 shows the
  stale version; this release updates it.

[#1]: https://github.com/anthropics/connect-rust/issues/1
[#2]: https://github.com/anthropics/connect-rust/issues/2
[#3]: https://github.com/anthropics/connect-rust/pull/3
[#4]: https://github.com/anthropics/connect-rust/pull/4

## [0.2.0] - 2026-03-17

First release from the [anthropics/connect-rust](https://github.com/anthropics/connect-rust)
repository. This is a complete from-scratch implementation — not a continuation
of the 0.1.x releases previously published under the `connectrpc` crate name,
which have been superseded.

### Protocol support

| Protocol | Server | Client |
|---|---|---|
| Connect (unary + streaming) | ✅ | ✅ |
| Connect GET (idempotent unary via query string) | ✅ | ✅ |
| gRPC over HTTP/2 | ✅ | ✅ |
| gRPC-Web | ✅ | ✅ |

| RPC type | Server | Client |
|---|---|---|
| Unary | ✅ | ✅ |
| Server streaming | ✅ | ✅ |
| Client streaming | ✅ | ✅ |
| Bidirectional streaming (full-duplex on h2, half-duplex on h1/h2) | ✅ | ✅ |

### Conformance

All applicable ConnectRPC conformance features are enabled. Test counts:

| Suite | Tests |
|---|---|
| Server (default) | 3600 |
| Server Connect+TLS (incl. mTLS) | 2396 |
| Client Connect (incl. GET, bidi, zstd, mTLS, h1 half-duplex) | 2580 |
| Client gRPC | 1454 |
| Client gRPC-Web | 2838 |

### Key features

**Runtime**
- Tower-based `ConnectRpcService<D>` — framework-agnostic, works with Axum, Hyper, etc.
- Monomorphic `FooServiceServer<T>` dispatcher (compile-time method dispatch, no `dyn Handler` vtable)
- Dynamic `Router` with runtime registration for multi-service or reflection use cases
- Pluggable compression via `CompressionProvider` trait; gzip + zstd built-in
- `#![deny(unsafe_code)]`, `#![warn(missing_docs)]`

**Client transports** (feature = `client`)
- `HttpClient::plaintext()` / `::with_tls()` — pooled hyper client, HTTP/1.1 + HTTP/2 via ALPN
- `Http2Connection::connect_plaintext()` / `::connect_tls()` — single raw h2 connection with
  honest `poll_ready`, composes with `tower::balance` for N-connection load spreading
- Security-first naming: no bare `::new()` — plaintext vs TLS is an explicit choice
- TLS accepts `Arc<rustls::ClientConfig>`, preserving dynamic cert rotation through
  `Arc<dyn ResolvesClientCert>`
- Whole-call deadline enforcement via `tokio::time::timeout_at` (gRPC semantics: deadline
  applies to the entire call, not per-message)

**Server** (feature = `server`)
- `Server::with_tls(Arc<rustls::ServerConfig>)` — mTLS via `with_client_cert_verifier()`
- Graceful shutdown with connection draining

**Generated clients**
- Dual methods per RPC: `foo(req)` (uses config defaults) + `foo_with_options(req, opts)`
- `ClientConfig` carries defaults for timeout, max message size, and headers — applied
  automatically by the no-options method

### Security

- **Message size limits enforced on both sides.** Request body collection,
  response body collection, envelope decoding, and decompression all apply
  configurable size limits, preventing either a malicious client or server
  from forcing unbounded memory allocation via oversized payloads or
  compression bombs.
- Both client and server default to 4 MiB per message
  (`DEFAULT_MAX_MESSAGE_SIZE`) when no explicit limit is configured — matching
  connect-go. Server: raise via `Limits::max_message_size`. Client: raise via
  `ClientConfig::default_max_message_size` or `CallOptions::max_message_size`.
- **TLS handshake timeout.** The server disconnects clients that open a TCP
  connection but stall the TLS handshake, preventing slowloris-style connection
  exhaustion. Defaults to 10 seconds (`DEFAULT_TLS_HANDSHAKE_TIMEOUT`);
  configure via `Server::tls_handshake_timeout`.
- **Timeout header digit-limit enforcement.** Per spec, `connect-timeout-ms`
  is capped at 10 digits and `grpc-timeout` at 8 digits (matching connect-go).
  Over-spec values are treated as no-timeout. Prevents a malicious client from
  triggering a per-request panic via `Instant + Duration` overflow. Deadline
  computation also uses `checked_add` as defense in depth.

### Code generation

- `connectrpc-codegen` — descriptor → Rust source library
- `connectrpc-build` — `build.rs` integration (protoc/buf → codegen → `OUT_DIR`)
- `protoc-gen-connect-rust` — protoc plugin binary

Generated code emits service traits, `FooServiceServer<T>` monomorphic dispatchers,
`FooServiceClient<T>` clients, and buffa message types via `buffa-codegen`.

### Not yet implemented

- gRPC server reflection
- HTTP/3 (blocked on hyper support)

### Performance

vs tonic 0.14 (same hyper/h2 stack), Intel Xeon 8488C:
- **1.95×** faster on small unary (single-request latency, no contention)
- **1.74×** faster on decode-heavy log ingest (50 records, ~15 KB)
- **~4%** ahead on realistic fortune+valkey workload (c=256)

The advantage comes from buffa's zero-copy view types (borrowed string fields
directly from the request buffer, no per-string alloc; `MapView` as flat
`Vec<(K,V)>` with no hashing) and compile-time dispatch via the generated
`FooServiceServer<T>`. See README for the full CPU breakdown.
