//! protoc-gen-connect-rust - Protoc plugin for generating ConnectRPC Rust bindings.
//!
//! This plugin generates:
//! - Server traits for implementing RPC handlers
//! - Service registration functions for the Router
//! - Client structs for making RPC calls
//!
//! Usage with protoc:
//!   protoc --connect-rust_out=. your_service.proto
//!
//! Usage with buf:
//!   Add to buf.gen.yaml:
//!   ```yaml
//!   plugins:
//!     - local: protoc-gen-connect-rust
//!       out: src/gen
//!   ```

use std::io;
use std::io::Read;
use std::io::Write;

use anyhow::Context;
use anyhow::Result;
use buffa::Message;

use connectrpc_codegen::codegen;
use connectrpc_codegen::plugin::CodeGeneratorRequest;

fn main() -> Result<()> {
    // Read the CodeGeneratorRequest from stdin
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .context("failed to read from stdin")?;

    let request = CodeGeneratorRequest::decode_from_slice(&input)
        .context("failed to decode CodeGeneratorRequest")?;

    // Process the request
    let response = codegen::generate(&request)?;

    // Write the CodeGeneratorResponse to stdout
    let output = response.encode_to_vec();
    io::stdout()
        .write_all(&output)
        .context("failed to write to stdout")?;

    Ok(())
}
