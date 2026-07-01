//! Encode and decode `google.rpc.Status` protobuf messages.
//!
//! The `google.rpc.Status` message is used in the `grpc-status-details-bin`
//! trailer to carry structured error details. Rather than generating the
//! message type from a `.proto` file, we use buffa's encoding primitives
//! directly since the message has only three fields:
//!
//! ```protobuf
//! message Status {
//!   int32 code = 1;
//!   string message = 2;
//!   repeated google.protobuf.Any details = 3;
//! }
//! ```

use buffa::encoding::{Tag, WireType, encode_varint};
use bytes::{Buf, BufMut, Bytes};

use crate::error::{ConnectError, ErrorDetail};

/// Encode a `ConnectError` as a `google.rpc.Status` protobuf message
/// for the `grpc-status-details-bin` trailer.
pub(crate) fn encode(err: &ConnectError) -> Bytes {
    let mut buf = Vec::new();

    // Field 1: int32 code
    Tag::new(1, WireType::Varint).encode(&mut buf);
    encode_varint(err.code.grpc_code() as u64, &mut buf);

    // Field 2: string message
    if let Some(ref message) = err.message {
        write_bytes_field(&mut buf, 2, message.as_bytes());
    }

    // Field 3: repeated Any details
    for detail in &err.details {
        let any_bytes = encode_any(&detail.type_url, &detail.value);
        write_bytes_field(&mut buf, 3, &any_bytes);
    }

    Bytes::from(buf)
}

/// Decode `ErrorDetail` entries from a `google.rpc.Status` protobuf message.
///
/// Only extracts the `repeated Any details` field (field 3); the `code` and
/// `message` fields are read from `grpc-status` / `grpc-message` trailers.
pub(crate) fn decode_details(data: &[u8]) -> Vec<ErrorDetail> {
    let mut details = Vec::new();
    let mut buf = data;

    while buf.has_remaining() {
        let Ok(tag) = Tag::decode(&mut buf) else {
            break;
        };

        match tag.wire_type() {
            WireType::Varint => {
                // Skip varint fields (code, etc.)
                if buffa::encoding::decode_varint(&mut buf).is_err() {
                    break;
                }
            }
            WireType::LengthDelimited => {
                let Ok(len) = buffa::encoding::decode_varint(&mut buf) else {
                    break;
                };
                let len = len as usize;
                if buf.remaining() < len {
                    break;
                }
                let field_data = &buf.chunk()[..len];

                if tag.field_number() == 3
                    && let Some(detail) = decode_any(field_data)
                {
                    details.push(detail);
                }

                buf.advance(len);
            }
            WireType::Fixed64 => {
                if buf.remaining() < 8 {
                    break;
                }
                buf.advance(8);
            }
            WireType::Fixed32 => {
                if buf.remaining() < 4 {
                    break;
                }
                buf.advance(4);
            }
            _ => break,
        }
    }

    details
}

/// Encode a `google.protobuf.Any` from a type URL and optional base64 value.
fn encode_any(type_url: &str, value: &Option<String>) -> Vec<u8> {
    let mut buf = Vec::new();

    // Field 1: string type_url. The Connect JSON `type` field carries the
    // bare fully-qualified name; a `google.protobuf.Any` wants a URL, so a
    // bare name gets the standard prefix here (an already-prefixed or
    // custom-host value passes through unchanged).
    if type_url.contains('/') {
        write_bytes_field(&mut buf, 1, type_url.as_bytes());
    } else {
        let prefixed = format!("type.googleapis.com/{type_url}");
        write_bytes_field(&mut buf, 1, prefixed.as_bytes());
    }

    // Field 2: bytes value (base64-decoded from ErrorDetail.value)
    if let Some(value_str) = value {
        match crate::error::detail_b64::decode_lenient(value_str) {
            Ok(value_bytes) => write_bytes_field(&mut buf, 2, &value_bytes),
            // Don't drop the detail silently — a hand-populated value in the
            // wrong alphabet (e.g. URL-safe base64) is a caller bug worth
            // surfacing; ErrorDetail::from_message cannot hit this path. The
            // type_url is still written, so the receiver sees which detail
            // was affected (indistinguishable from a legitimately empty
            // message of that type — the warning is the disambiguator).
            Err(_) => tracing::warn!(
                type_url,
                "error detail value is not valid base64; omitting it from \
                 the gRPC status (use ErrorDetail::from_message to encode)"
            ),
        }
    }

    buf
}

/// Decode a `google.protobuf.Any` message into an `ErrorDetail`.
fn decode_any(data: &[u8]) -> Option<ErrorDetail> {
    let mut type_url = None;
    let mut value = None;
    let mut buf = data;

    while buf.has_remaining() {
        let tag = Tag::decode(&mut buf).ok()?;

        match tag.wire_type() {
            WireType::LengthDelimited => {
                let len = buffa::encoding::decode_varint(&mut buf).ok()? as usize;
                if buf.remaining() < len {
                    break;
                }
                let field_data = &buf.chunk()[..len];

                match tag.field_number() {
                    1 => type_url = Some(std::str::from_utf8(field_data).ok()?.to_owned()),
                    2 => value = Some(field_data.to_vec()),
                    _ => {}
                }

                buf.advance(len);
            }
            WireType::Varint => {
                buffa::encoding::decode_varint(&mut buf).ok()?;
            }
            _ => break,
        }
    }

    Some(ErrorDetail {
        type_url: type_url?,
        value: Some(crate::error::detail_b64::encode(&value.unwrap_or_default())),
        debug: None,
    })
}

/// Write a length-delimited protobuf field (wire type 2).
fn write_bytes_field(buf: &mut Vec<u8>, field_number: u32, data: &[u8]) {
    Tag::new(field_number, WireType::LengthDelimited).encode(buf);
    encode_varint(data.len() as u64, buf);
    buf.put_slice(data);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn test_encode_decode_roundtrip() {
        let err = ConnectError::new(ErrorCode::Internal, "test error");
        let encoded = encode(&err);
        let details = decode_details(&encoded);
        // No details on the error, so decoded details should be empty
        assert!(details.is_empty());
    }

    #[test]
    fn test_encode_decode_with_details() {
        use base64::Engine;

        let detail = ErrorDetail {
            type_url: "type.googleapis.com/test.Detail".to_string(),
            value: Some(base64::engine::general_purpose::STANDARD_NO_PAD.encode(b"\x01\x02\x03")),
            debug: None,
        };
        let err = ConnectError::new(ErrorCode::NotFound, "not found").with_detail(detail);

        let encoded = encode(&err);
        let details = decode_details(&encoded);
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].type_url, "type.googleapis.com/test.Detail");
        let value_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(details[0].value.as_ref().unwrap())
            .unwrap();
        assert_eq!(value_bytes, b"\x01\x02\x03");
    }

    #[test]
    fn from_message_detail_survives_grpc_roundtrip() {
        use base64::Engine;
        use buffa_types::google::protobuf::StringValue;

        // The supported path: bare type name in, value carried on the wire,
        // and the Any's type_url gains the standard prefix.
        let msg = StringValue::from("retry in 3s");
        let detail = ErrorDetail::from_message("google.protobuf.StringValue", &msg);
        let err = ConnectError::new(ErrorCode::Unavailable, "overloaded").with_detail(detail);

        let details = decode_details(&encode(&err));
        assert_eq!(details.len(), 1);
        assert_eq!(
            details[0].type_url,
            "type.googleapis.com/google.protobuf.StringValue"
        );
        let value_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(details[0].value.as_ref().unwrap())
            .unwrap();
        assert_eq!(value_bytes, buffa::Message::encode_to_vec(&msg));
    }

    #[test]
    fn undecodable_detail_value_is_omitted_not_corrupted() {
        // A hand-populated value in a wrong alphabet: the detail's type_url
        // still encodes, the value field is omitted (and warned about),
        // and the rest of the status survives.
        let detail = ErrorDetail {
            type_url: "type.googleapis.com/test.Detail".to_string(),
            value: Some("not-base64!!!".to_string()),
            debug: None,
        };
        let err = ConnectError::new(ErrorCode::Internal, "boom").with_detail(detail);
        let details = decode_details(&encode(&err));
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].type_url, "type.googleapis.com/test.Detail");
        assert_eq!(details[0].value.as_deref(), Some(""));
    }

    #[test]
    fn test_decode_empty() {
        assert!(decode_details(&[]).is_empty());
    }

    #[test]
    fn test_decode_skips_non_details_fields() {
        // Status with code=13 (field 1, varint) and message="err" (field 2, string)
        // but no details (field 3)
        let buf = vec![
            0x08, 13, // field 1 varint: code = 13
            0x12, 3, b'e', b'r', b'r', // field 2 string: message = "err"
        ];
        assert!(decode_details(&buf).is_empty());
    }

    #[test]
    fn test_encode_includes_code_and_message() {
        let err = ConnectError::new(ErrorCode::Unavailable, "overloaded");
        let encoded = encode(&err);
        // Verify the encoded bytes start with field 1 (code = 14 for Unavailable)
        assert!(encoded.len() > 2);
        assert_eq!(encoded[0], 0x08); // tag: field 1, varint
        assert_eq!(encoded[1], 14); // Unavailable = 14
    }

    #[test]
    fn test_decode_truncated() {
        // Truncated data should not panic
        assert!(decode_details(&[0x1A]).is_empty());
        assert!(decode_details(&[0x1A, 0x80]).is_empty());
    }
}
