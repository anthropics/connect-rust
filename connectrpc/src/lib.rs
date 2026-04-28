//! ConnectRPC implementation for Rust
//!
//! This crate provides a tower-based ConnectRPC runtime that can be integrated
//! with any HTTP framework that supports tower services (axum, hyper, tonic, etc.).
//!
//! # Architecture
//!
//! The core abstraction is [`ConnectRpcService`], a [`tower::Service`] that handles
//! ConnectRPC requests. This allows seamless integration with existing web servers:
//!
//! ```rust,ignore
//! use connectrpc::{Router, ConnectRpcService};
//! use std::sync::Arc;
//!
//! // Build your router with RPC handlers
//! let greet_impl = Arc::new(MyGreetService);
//! let router = greet_impl.register(Router::new());
//!
//! // Get a tower::Service - use with ANY compatible framework
//! let service = ConnectRpcService::new(router);
//! ```
//!
//! # Framework Integration
//!
//! ## With Axum (recommended)
//!
//! Enable the `axum` feature for convenient integration:
//!
//! ```rust,ignore
//! use axum::{Router, routing::get};
//! use connectrpc::Router as ConnectRouter;
//! use std::sync::Arc;
//!
//! let greet_impl = Arc::new(MyGreetService);
//! let connect = greet_impl.register(ConnectRouter::new());
//!
//! let app = Router::new()
//!     .route("/health", get(health))
//!     .fallback_service(connect.into_axum_service());
//!
//! axum::serve(listener, app).await?;
//! ```
//!
//! ## With Raw Hyper
//!
//! Use `ConnectRpcService` directly with hyper's service machinery.
//!
//! ## Standalone Server
//!
//! For simple cases, enable the `server` feature for a built-in hyper server:
//!
//! ```rust,ignore
//! use connectrpc::{Router, Server};
//!
//! let router = Router::new();
//! // ... register handlers ...
//!
//! Server::new(router).serve(addr).await?;
//! ```
//!
//! # Modules
//!
//! - [`codec`] - Message encoding/decoding (protobuf and JSON)
//! - [`compression`] - Pluggable compression (gzip, zstd) with streaming support
//! - [`envelope`] - Streaming message framing (5-byte header + payload)
//! - [`error`] - ConnectRPC error types and HTTP status mapping
//! - [`handler`] - Async handler traits for implementing RPC methods
//! - [`router`] - Request routing and service registration
//! - [`service`] - Tower service implementation (primary integration point)
//! - [`client`] - Tower-based HTTP client utilities (requires `client` feature)
//! - [`server`] - Standalone hyper-based server (requires `server` feature)
//!
//! # Protocol Support
//!
//! This implementation follows the ConnectRPC protocol specification:
//! - Unary RPC calls (request-response)
//! - Proto and JSON message encoding
//! - Compression negotiation (gzip, zstd) with streaming support
//! - Error handling with proper HTTP status mapping
//! - Trailers via `trailer-` prefixed headers
//! - Envelope framing for streaming messages
//!
//! # Client
//!
//! Enable the `client` feature and use generated clients with a transport.
//!
//! **For gRPC** (HTTP/2), use [`Http2Connection`](client::Http2Connection):
//!
//! ```rust,ignore
//! use connectrpc::client::{Http2Connection, ClientConfig};
//! use connectrpc::Protocol;
//!
//! let uri: http::Uri = "http://localhost:8080".parse()?;
//! let conn = Http2Connection::connect_plaintext(uri.clone()).await?.shared(1024);
//! let config = ClientConfig::new(uri).protocol(Protocol::Grpc);
//!
//! let client = GreetServiceClient::new(conn, config);
//! let response = client.greet(request).await?;
//! ```
//!
//! **For Connect over HTTP/1.1** (or unknown protocol), use
//! [`HttpClient`](client::HttpClient):
//!
//! ```rust,ignore
//! use connectrpc::client::{HttpClient, ClientConfig};
//!
//! let http = HttpClient::plaintext();  // cleartext http:// only
//! let config = ClientConfig::new("http://localhost:8080".parse()?);
//!
//! let client = GreetServiceClient::new(http, config);
//! ```
//!
//! ## Per-call options and defaults
//!
//! Generated clients expose both `foo(req)` and `foo_with_options(req, opts)`
//! for each RPC. Use [`CallOptions`](client::CallOptions) for per-call timeout,
//! headers, message-size limits, and compression overrides.
//!
//! For settings you want on every call, configure [`ClientConfig`](client::ClientConfig)
//! defaults — they're applied automatically by the no-options method:
//!
//! ```rust,ignore
//! let config = ClientConfig::new(uri)
//!     .default_timeout(Duration::from_secs(30))
//!     .default_header("authorization", "Bearer ...");
//!
//! let client = GreetServiceClient::new(http, config);
//! client.greet(req).await?;  // uses 30s timeout + auth header
//! ```
//!
//! Per-call `CallOptions` override config defaults.
//!
//! See the [`client`] module docs for connection balancing and the
//! transport selection rationale.
//!
//! # Feature Flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `gzip` | ✓ | Gzip compression |
//! | `zstd` | ✓ | Zstandard compression |
//! | `streaming` | ✓ | Streaming compression support |
//! | `client` | ✗ | HTTP client transports (plaintext) |
//! | `client-tls` | ✗ | TLS for client transports |
//! | `server` | ✗ | Standalone hyper-based server |
//! | `server-tls` | ✗ | TLS for the built-in server |
//! | `tls` | ✗ | Convenience: `server-tls` + `client-tls` |
//! | `axum` | ✗ | Axum framework integration |

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// Core modules (always available)
pub mod codec;
pub mod compression;
pub mod dispatcher;
pub mod envelope;
pub mod error;
pub(crate) mod grpc_status;
pub mod handler;
pub mod protocol;
pub mod response;
pub mod router;
pub mod service;

// Optional: HTTP client
pub mod client;

// Optional: Standalone hyper-based server
#[cfg(feature = "server")]
pub mod server;

// ============================================================================
// Primary exports - Tower-first API
// ============================================================================

// The main entry point - a tower::Service for ConnectRPC
pub use service::ConnectRpcBody;
pub use service::ConnectRpcService;
pub use service::Limits;
pub use service::StreamingResponseBody;

// Router for registering RPC handlers
pub use router::MethodKind;
pub use router::Router;

// Dispatcher trait for monomorphic dispatch (codegen-backed alternative to Router)
pub use dispatcher::Chain;
pub use dispatcher::Dispatcher;
pub use dispatcher::MethodDescriptor;

// Handler traits and request/response types
pub use handler::BidiStreamingHandler;
pub use handler::ClientStreamingHandler;
pub use handler::Handler;
pub use handler::StreamingHandler;
pub use handler::ViewBidiStreamingHandler;
pub use handler::ViewClientStreamingHandler;
pub use handler::ViewHandler;
pub use handler::ViewStreamingHandler;
pub use handler::bidi_streaming_handler_fn;
pub use handler::client_streaming_handler_fn;
pub use handler::handler_fn;
pub use handler::streaming_handler_fn;
pub use handler::view_bidi_streaming_handler_fn;
pub use handler::view_client_streaming_handler_fn;
pub use handler::view_handler_fn;
pub use handler::view_streaming_handler_fn;
pub use response::Encodable;
pub use response::EncodedResponse;
pub use response::RequestContext;
pub use response::Response;
pub use response::ServiceResult;
pub use response::ServiceStream;

// Error types
pub use error::ConnectError;
pub use error::ErrorCode;

// Protocol detection
pub use protocol::Protocol;
pub use protocol::RequestProtocol;

// ============================================================================
// Codec exports
// ============================================================================

pub use codec::CodecFormat;
pub use codec::JsonCodec;
pub use codec::ProtoCodec;

// ============================================================================
// Compression exports
// ============================================================================

pub use compression::CompressionPolicy;
pub use compression::CompressionProvider;
pub use compression::CompressionRegistry;
pub use compression::DEFAULT_COMPRESSION_MIN_SIZE;

#[cfg(feature = "gzip")]
pub use compression::GzipProvider;

#[cfg(feature = "zstd")]
pub use compression::ZstdProvider;

#[cfg(feature = "streaming")]
pub use compression::BoxedAsyncBufRead;

#[cfg(feature = "streaming")]
pub use compression::BoxedAsyncRead;

#[cfg(feature = "streaming")]
pub use compression::StreamingCompressionProvider;

// ============================================================================
// Optional: Standalone server
// ============================================================================

#[cfg(feature = "server")]
pub use server::BoundServer;

#[cfg(feature = "server")]
pub use server::Server;

#[cfg(feature = "server")]
pub use server::PeerAddr;
#[cfg(feature = "server-tls")]
pub use server::PeerCerts;

/// Re-export of `rustls` for TLS configuration.
///
/// Use this to construct a [`rustls::ServerConfig`] for [`Server::with_tls`]
/// or a [`rustls::ClientConfig`] for [`HttpClient::with_tls`](client::HttpClient::with_tls)
/// / [`Http2Connection::connect_tls`](client::Http2Connection::connect_tls).
#[cfg(any(feature = "server-tls", feature = "client-tls"))]
pub use rustls;
