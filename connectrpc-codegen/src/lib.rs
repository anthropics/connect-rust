//! connectrpc-codegen — library for generating ConnectRPC Rust bindings.
//!
//! This crate provides programmatic code generation from compiled proto
//! descriptors: buffa message types + ConnectRPC service traits, extension
//! traits, and typed clients.
//!
//! Most users will not use this crate directly. Use either:
//!
//! - **`protoc-gen-connect-rust`** — protoc/buf plugin binary (generates
//!   checked-in code via `buf generate` or `protoc --connect-rust_out=.`)
//! - **`connectrpc-build`** — `build.rs` integration (generates code at
//!   build time into `$OUT_DIR`)
//!
//! # Library usage
//!
//! Lower-level consumers (e.g. a custom build pipeline or a plugin that
//! wraps this one) can call [`codegen::generate_files`] directly:
//!
//! ```rust,ignore
//! use connectrpc_codegen::codegen::{generate_files, Options};
//!
//! let files = generate_files(&descriptors, &files_to_generate, &Options::default())?;
//! for f in files {
//!     std::fs::write(out_dir.join(&f.name), f.content)?;
//! }
//! ```

pub mod codegen;
pub mod plugin;
