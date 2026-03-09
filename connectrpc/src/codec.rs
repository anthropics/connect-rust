//! Message encoding and decoding for ConnectRPC.
//!
//! This module provides codec implementations for serializing and deserializing
//! protobuf messages in both binary proto and JSON formats.

use buffa::Message;
use bytes::Bytes;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::ConnectError;

/// Content types supported by ConnectRPC.
pub mod content_type {
    /// Binary protobuf content type.
    pub const PROTO: &str = "application/proto";
    /// JSON content type.
    pub const JSON: &str = "application/json";
    /// Connect streaming proto content type.
    pub const CONNECT_PROTO: &str = "application/connect+proto";
    /// Connect streaming JSON content type.
    pub const CONNECT_JSON: &str = "application/connect+json";
}

/// Connect protocol header names.
pub mod header {
    /// Declares the Connect protocol version (always `"1"`).
    pub const PROTOCOL_VERSION: &str = "connect-protocol-version";
    /// Request timeout in milliseconds.
    pub const TIMEOUT_MS: &str = "connect-timeout-ms";
    /// Content encoding for Connect streaming requests/responses.
    pub const CONTENT_ENCODING: &str = "connect-content-encoding";
    /// Accepted content encodings for Connect streaming requests/responses.
    pub const ACCEPT_ENCODING: &str = "connect-accept-encoding";
}

/// Encode a protobuf message to binary format.
pub fn encode_proto<M: Message>(message: &M) -> Result<Bytes, ConnectError> {
    Ok(message.encode_to_bytes())
}

/// Decode bytes into a protobuf message.
pub fn decode_proto<M: Message>(data: &[u8]) -> Result<M, ConnectError> {
    M::decode_from_slice(data)
        .map_err(|e| ConnectError::invalid_argument(format!("failed to decode proto: {e}")))
}

/// Encode a message to JSON format.
pub fn encode_json<M: Serialize>(message: &M) -> Result<Bytes, ConnectError> {
    serde_json::to_vec(message)
        .map(Bytes::from)
        .map_err(|e| ConnectError::internal(format!("failed to encode JSON: {e}")))
}

/// Decode JSON bytes into a message.
pub fn decode_json<M: DeserializeOwned>(data: &[u8]) -> Result<M, ConnectError> {
    serde_json::from_slice(data)
        .map_err(|e| ConnectError::invalid_argument(format!("failed to decode JSON: {e}")))
}

/// Codec for binary protobuf encoding.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProtoCodec;

impl ProtoCodec {
    /// Get the content type for this codec.
    pub fn content_type() -> &'static str {
        content_type::PROTO
    }

    /// Encode a protobuf message to bytes.
    pub fn encode<M: Message>(message: &M) -> Result<Bytes, ConnectError> {
        encode_proto(message)
    }

    /// Decode bytes into a protobuf message.
    pub fn decode<M: Message>(data: &[u8]) -> Result<M, ConnectError> {
        decode_proto(data)
    }
}

/// Codec for JSON encoding of protobuf messages.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonCodec;

impl JsonCodec {
    /// Get the content type for this codec.
    pub fn content_type() -> &'static str {
        content_type::JSON
    }

    /// Encode a message to JSON bytes.
    pub fn encode<M: Serialize>(message: &M) -> Result<Bytes, ConnectError> {
        encode_json(message)
    }

    /// Decode JSON bytes into a message.
    pub fn decode<M: DeserializeOwned>(data: &[u8]) -> Result<M, ConnectError> {
        decode_json(data)
    }
}

/// Supported codec formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CodecFormat {
    /// Binary protobuf format.
    Proto,
    /// JSON format.
    Json,
}

impl std::fmt::Display for CodecFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Proto => write!(f, "proto"),
            Self::Json => write!(f, "json"),
        }
    }
}

impl CodecFormat {
    /// Parse codec format from content type string.
    pub fn from_content_type(content_type: &str) -> Option<Self> {
        if content_type.starts_with(content_type::PROTO)
            || content_type.starts_with(content_type::CONNECT_PROTO)
        {
            Some(Self::Proto)
        } else if content_type.starts_with(content_type::JSON)
            || content_type.starts_with(content_type::CONNECT_JSON)
        {
            Some(Self::Json)
        } else {
            None
        }
    }

    /// Parse codec format from encoding name (used in GET request query params).
    ///
    /// Accepts "proto" or "json" (the values used in the `encoding` query parameter).
    pub fn from_codec(codec: &str) -> Option<Self> {
        match codec {
            "proto" => Some(Self::Proto),
            "json" => Some(Self::Json),
            _ => None,
        }
    }

    /// Get the content type string for this format (unary RPC).
    #[inline]
    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Proto => content_type::PROTO,
            Self::Json => content_type::JSON,
        }
    }

    /// Get the streaming content type string for this format.
    #[inline]
    pub fn streaming_content_type(&self) -> &'static str {
        match self {
            Self::Proto => content_type::CONNECT_PROTO,
            Self::Json => content_type::CONNECT_JSON,
        }
    }

    /// Check if the given content type indicates a streaming request.
    #[inline]
    pub fn is_streaming_content_type(content_type: &str) -> bool {
        content_type.starts_with(content_type::CONNECT_PROTO)
            || content_type.starts_with(content_type::CONNECT_JSON)
    }
}
