//! Protoc plugin protocol types.
//!
//! Re-exports the CodeGeneratorRequest/Response types from buffa-codegen's
//! bootstrapped descriptor types. These are buffa-generated message structs
//! that implement `buffa::Message` for decoding/encoding the protoc plugin
//! protocol.

pub use buffa_codegen::generated::compiler::CodeGeneratorRequest;
pub use buffa_codegen::generated::compiler::CodeGeneratorResponse;
pub use buffa_codegen::generated::compiler::code_generator_response::Feature as CodeGeneratorResponseFeature;
pub use buffa_codegen::generated::compiler::code_generator_response::File as CodeGeneratorResponseFile;
