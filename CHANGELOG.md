# Changelog

All notable changes to connectrpc will be documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
with the [Rust 0.x convention](https://doc.rust-lang.org/cargo/reference/semver.html):
breaking changes increment the minor version (0.2 → 0.3), additive changes
increment the patch version.

## [Unreleased]

### Added

- **`emit_register_fn` option** on `connectrpc_codegen::codegen::Options` and
  `connectrpc_build::Config`, plumbing through to
  `buffa_codegen::CodeGenConfig::emit_register_fn`. Set to `false` to suppress
  the per-file `register_types(&mut TypeRegistry)` aggregator when multiple
  generated files are `include!`d into the same module (the identically-named
  functions would otherwise collide). The protoc plugin accepts a matching
  `no_register_fn` parameter for path-compat with the unified `connectrpc-build`
  flow.

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
