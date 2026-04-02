//! Bazel-built connectrpc example.
//!
//! Demonstrates that protoc plugins (`protoc-gen-buffa`,
//! `protoc-gen-buffa-packaging`, `protoc-gen-connect-rust`) can drive code
//! generation from a Bazel build, with no `env!`-baked paths and no
//! checked-in generated code.
//!
//! The two `#[path = "..."]` mounts below point at generated files
//! produced by the `//:gen_code` Bazel target — they live in the Bazel
//! output tree and are wired into this `rust_library` as srcs.

#[path = "../generated/buffa/mod.rs"]
pub mod proto;

#[path = "../generated/connect/mod.rs"]
pub mod connect;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_types_are_constructible() {
        let request = proto::anthropic::connectrpc::examples::greet::v1::GreetRequest {
            name: "world".into(),
            ..Default::default()
        };
        assert_eq!(request.name, "world");
    }

    #[test]
    fn message_types_round_trip_through_buffa() {
        use buffa::Message;

        let original = proto::anthropic::connectrpc::examples::greet::v1::GreetResponse {
            message: "Hello, world!".into(),
            ..Default::default()
        };
        let bytes = original.encode_to_vec();
        let decoded = proto::anthropic::connectrpc::examples::greet::v1::GreetResponse::decode_from_slice(&bytes)
            .expect("decode round-trip");
        assert_eq!(decoded.message, "Hello, world!");
    }

    #[test]
    fn service_name_constant_is_correct() {
        assert_eq!(
            connect::anthropic::connectrpc::examples::greet::v1::GREET_SERVICE_SERVICE_NAME,
            "anthropic.connectrpc.examples.greet.v1.GreetService"
        );
    }
}
