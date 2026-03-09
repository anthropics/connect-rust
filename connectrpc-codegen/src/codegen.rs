//! Code generation logic for ConnectRPC Rust bindings.
//!
//! This module generates:
//! - Buffa message types (via buffa-codegen)
//! - ConnectRPC service traits and clients
//!
//! Code generation uses the `quote` crate for producing Rust code from
//! TokenStreams, which provides better syntax highlighting, type safety,
//! and maintainability compared to string-based generation.

use anyhow::Result;
use heck::ToSnakeCase;
use heck::ToUpperCamelCase;
use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;

use buffa_codegen::generated::descriptor::FileDescriptorProto;
use buffa_codegen::generated::descriptor::MethodDescriptorProto;
use buffa_codegen::generated::descriptor::ServiceDescriptorProto;
use buffa_codegen::generated::descriptor::SourceCodeInfo;
use buffa_codegen::generated::descriptor::method_options::IdempotencyLevel;

pub use buffa_codegen::GeneratedFile;
pub use buffa_codegen::generated::descriptor;

use crate::plugin::CodeGeneratorRequest;
use crate::plugin::CodeGeneratorResponse;
use crate::plugin::CodeGeneratorResponseFile;

/// Options for ConnectRPC code generation.
///
/// These control both the underlying buffa message generation and the
/// ConnectRPC service binding generation.
///
/// Construct via `Options { field: value, ..Options::default() }`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Options {
    /// Emit `Vec<u8>`/`&[u8]` for proto string fields with
    /// `utf8_validation = NONE` instead of `String`/`&str`. See
    /// `buffa_codegen::CodeGenConfig::strict_utf8_mapping`.
    pub strict_utf8_mapping: bool,
    /// Emit `serde::Serialize` / `serde::Deserialize` derives and the proto3
    /// JSON mapping helpers on generated message types. Required for the
    /// Connect protocol's JSON codec; disable only if you're targeting
    /// binary-only clients.
    pub generate_json: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            strict_utf8_mapping: false,
            generate_json: true,
        }
    }
}

impl Options {
    fn to_buffa_config(&self) -> buffa_codegen::CodeGenConfig {
        let mut config = buffa_codegen::CodeGenConfig::default();
        config.generate_views = true;
        config.generate_json = self.generate_json;
        config.strict_utf8_mapping = self.strict_utf8_mapping;
        config
    }
}

/// Generate ConnectRPC service bindings + buffa message types from proto
/// descriptors.
///
/// Returns one `GeneratedFile` per proto file in `file_to_generate`. Does
/// **not** emit a `mod.rs` — callers assemble the module tree themselves
/// (the protoc plugin via [`buffa_codegen::generate_module_tree`], build.rs
/// integrations via an `include!`-based file).
///
/// # Errors
///
/// Returns an error if buffa-codegen fails (e.g. unsupported proto
/// feature) or if the generated service binding Rust does not parse
/// under `syn` (indicates a bug in this crate).
pub fn generate_files(
    proto_file: &[FileDescriptorProto],
    file_to_generate: &[String],
    options: &Options,
) -> Result<Vec<GeneratedFile>> {
    let config = options.to_buffa_config();

    // 1. Generate message types via buffa-codegen
    let mut files = buffa_codegen::generate(proto_file, file_to_generate, &config)
        .map_err(|e| anyhow::anyhow!("buffa-codegen failed: {e}"))?;

    // Build the type resolver once for all services. It mirrors the
    // CodeGenContext that buffa_codegen::generate() built internally,
    // so cross-package / WKT / nested type paths resolve identically.
    let resolver = TypeResolver::new(proto_file, file_to_generate, &config);

    // 2. Generate ConnectRPC service bindings and append to matching files
    for file_name in file_to_generate {
        let file_desc = proto_file
            .iter()
            .find(|f| f.name.as_deref() == Some(file_name.as_str()));

        if let Some(file) = file_desc
            && !file.service.is_empty()
        {
            let service_tokens = generate_connect_services(file, &resolver)?;
            let service_code = format_token_stream(&service_tokens)?;

            let rust_filename = buffa_codegen::proto_path_to_rust_module(file_name);
            if let Some(out) = files.iter_mut().find(|g| g.name == rust_filename) {
                out.content.push('\n');
                out.content.push_str(&service_code);
            }
        }
    }

    Ok(files)
}

/// Generate a `CodeGeneratorResponse` from a protoc `CodeGeneratorRequest`.
///
/// This is the entry point for the protoc plugin (`protoc-gen-connect-rust`).
/// It parses the comma-separated `request.parameter` into [`Options`],
/// delegates to [`generate_files`], and adds a `mod.rs` that assembles the
/// generated files into a module tree.
pub fn generate(request: &CodeGeneratorRequest) -> Result<CodeGeneratorResponse> {
    let mut options = Options::default();

    // Parse plugin options from the comma-separated `parameter` string.
    // Example: buf.gen.yaml `opt: ["strict_utf8_mapping"]`
    // → protoc `--plugin_opt=strict_utf8_mapping` → request.parameter.
    if let Some(ref param) = request.parameter {
        for opt in param.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match opt {
                "strict_utf8_mapping" => options.strict_utf8_mapping = true,
                "no_json" => options.generate_json = false,
                _ => {
                    return Err(anyhow::anyhow!(
                        "unknown plugin option: {opt:?}. Supported: strict_utf8_mapping, no_json"
                    ));
                }
            }
        }
    }

    let generated = generate_files(&request.proto_file, &request.file_to_generate, &options)?;

    // Build the mod.rs module tree from (file-name, package) pairs.
    let entries: Vec<(&str, &str)> = generated
        .iter()
        .map(|g| {
            let package = request
                .proto_file
                .iter()
                .find(|fd| {
                    fd.name
                        .as_deref()
                        .is_some_and(|n| buffa_codegen::proto_path_to_rust_module(n) == g.name)
                })
                .and_then(|fd| fd.package.as_deref())
                .unwrap_or("");
            (g.name.as_str(), package)
        })
        .collect();

    let mod_content = buffa_codegen::generate_module_tree(&entries, "", true);

    // Build CodeGeneratorResponse
    let mut files: Vec<CodeGeneratorResponseFile> = generated
        .into_iter()
        .map(|g| CodeGeneratorResponseFile {
            name: Some(g.name),
            content: Some(g.content),
            ..Default::default()
        })
        .collect();

    // Add mod.rs
    files.push(CodeGeneratorResponseFile {
        name: Some("mod.rs".to_string()),
        content: Some(mod_content),
        ..Default::default()
    });

    Ok(CodeGeneratorResponse {
        supported_features: Some(feature_flags()),
        minimum_edition: Some(EDITION_2023),
        maximum_edition: Some(EDITION_2023),
        file: files,
        ..Default::default()
    })
}

/// Feature flags we support (bitmask). See
/// `google.protobuf.compiler.CodeGeneratorResponse.Feature`.
fn feature_flags() -> u64 {
    const FEATURE_PROTO3_OPTIONAL: u64 = 1;
    const FEATURE_SUPPORTS_EDITIONS: u64 = 2;
    FEATURE_PROTO3_OPTIONAL | FEATURE_SUPPORTS_EDITIONS
}

/// Edition 2023 numeric value. buffa-codegen handles proto2/proto3/edition-2023;
/// we declare 2023 as both min and max.
const EDITION_2023: i32 = 1000;

/// Format a TokenStream into a Rust source string via prettyplease.
fn format_token_stream(tokens: &TokenStream) -> Result<String> {
    let file = syn::parse2::<syn::File>(tokens.clone())
        .map_err(|e| anyhow::anyhow!("generated code failed to parse: {e}"))?;
    Ok(prettyplease::unparse(&file))
}

/// Emit `#[doc = " line"]` attributes for each line of `text`.
///
/// prettyplease renders `#[doc = "X"]` as `///X` verbatim (no space inserted);
/// to get `/// X` the string must already start with a space. This helper
/// prefixes each line with a space so the unparsed output matches hand-written
/// doc comment style.
///
/// Leaves blank lines as-is (→ `///`) so paragraph breaks render correctly.
fn doc_attrs(text: &str) -> TokenStream {
    let lines: Vec<String> = text
        .lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!(" {l}")
            }
        })
        .collect();
    quote! { #(#[doc = #lines])* }
}

// ---------------------------------------------------------------------------
// Type path resolution
// ---------------------------------------------------------------------------

/// Resolves fully-qualified protobuf type names to Rust type-path tokens
/// relative to the current file's package module.
///
/// Wraps [`buffa_codegen::context::CodeGenContext`] via `for_generate()` so
/// service method input/output types resolve to the same paths buffa-codegen
/// emits for message fields — including cross-package (`super::foo::Bar`),
/// WKT extern paths (`::buffa_types::google::protobuf::Empty`), and nested
/// types (`outer::Inner`). Zero drift with buffa's own generation.
struct TypeResolver<'a> {
    ctx: buffa_codegen::context::CodeGenContext<'a>,
}

impl<'a> TypeResolver<'a> {
    fn new(
        proto_file: &'a [FileDescriptorProto],
        file_to_generate: &[String],
        config: &'a buffa_codegen::CodeGenConfig,
    ) -> Self {
        Self {
            ctx: buffa_codegen::context::CodeGenContext::for_generate(
                proto_file,
                file_to_generate,
                config,
            ),
        }
    }

    /// Resolve a proto FQN (e.g. `.google.protobuf.Empty`) to Rust type-path
    /// tokens relative to `current_package`. Falls back to the bare type name
    /// if the type is not in the context (malformed proto or missing import).
    fn rust_type(&self, proto_fqn: &str, current_package: &str) -> TokenStream {
        if let Some(path) = self.ctx.rust_type_relative(proto_fqn, current_package, 0) {
            return buffa_codegen::idents::rust_path_to_tokens(&path);
        }
        // Fallback: unknown type. Emit the bare identifier and let rustc
        // produce a meaningful compile error pointing at the generated code.
        let bare = bare_type_name(proto_fqn);
        let ident = format_ident!("{bare}");
        quote! { #ident }
    }

    /// Like `rust_type` but appends `View` to the last path segment, e.g.
    /// `super::foo::Bar` → `super::foo::BarView`.
    fn rust_view_type(&self, proto_fqn: &str, current_package: &str) -> TokenStream {
        if let Some(path) = self.ctx.rust_type_relative(proto_fqn, current_package, 0) {
            return buffa_codegen::idents::rust_path_to_tokens(&format!("{path}View"));
        }
        let bare = bare_type_name(proto_fqn);
        let ident = format_ident!("{bare}View");
        quote! { #ident }
    }
}

/// Last segment of a proto FQN, e.g. `.google.protobuf.Empty` → `"Empty"`.
/// Fallback for types absent from the resolver context.
fn bare_type_name(proto_fqn: &str) -> &str {
    proto_fqn
        .strip_prefix('.')
        .unwrap_or(proto_fqn)
        .rsplit('.')
        .next()
        .unwrap_or(proto_fqn)
}

// ---------------------------------------------------------------------------
// ConnectRPC service code generation
// ---------------------------------------------------------------------------

/// Generate ConnectRPC service bindings for a file.
fn generate_connect_services(
    file: &FileDescriptorProto,
    resolver: &TypeResolver<'_>,
) -> Result<TokenStream> {
    let mut tokens = TokenStream::new();

    // All crate-level imports use `::connectrpc` (absolute path) so that
    // proto packages named `connectrpc.*` (e.g. `connectrpc.conformance.v1`)
    // don't shadow the crate in generated module scopes.
    let imports = quote! {
        use std::future::Future;
        use std::pin::Pin;
        use std::sync::Arc;

        use ::connectrpc::{Context, ConnectError, Router, Dispatcher, view_handler_fn, view_streaming_handler_fn, view_client_streaming_handler_fn, view_bidi_streaming_handler_fn};
        use ::connectrpc::dispatcher::codegen as __crpc_codegen;
        use ::connectrpc::CodecFormat as __CodecFormat;
        use buffa::bytes::Bytes as __Bytes;
        use ::connectrpc::client::{ClientConfig, ClientTransport, CallOptions, call_unary, call_server_stream, call_client_stream, call_bidi_stream};
        use futures::Stream;
        use buffa::Message;
        use buffa::view::OwnedView;
    };
    tokens.extend(imports);

    for service in &file.service {
        tokens.extend(generate_service(file, service, resolver)?);
    }

    Ok(tokens)
}

/// Generate code for a single service.
fn generate_service(
    file: &FileDescriptorProto,
    service: &ServiceDescriptorProto,
    resolver: &TypeResolver<'_>,
) -> Result<TokenStream> {
    let package = file.package.as_deref().unwrap_or("");
    let service_name = service.name.as_deref().unwrap_or("");
    // Empty package is valid proto; the fully-qualified service name is just
    // `ServiceName`, not `.ServiceName` (which would break interop).
    let full_service_name = if package.is_empty() {
        service_name.to_string()
    } else {
        format!("{package}.{service_name}")
    };
    let trait_name = format_ident!("{}", service_name.to_upper_camel_case());
    let ext_trait_name = format_ident!("{}Ext", service_name.to_upper_camel_case());
    let client_name = format_ident!("{}Client", service_name.to_upper_camel_case());
    let service_name_const = format_ident!(
        "{}_SERVICE_NAME",
        service_name.to_snake_case().to_uppercase()
    );

    // Get service documentation and append async impl guidance
    let service_doc = get_service_comment(file, service).unwrap_or_default();
    let base_doc = if service_doc.is_empty() {
        format!("Server trait for {service_name}.")
    } else {
        service_doc
    };
    let full_doc = format!(
        "{base_doc}\n\n\
         # Implementing handlers\n\n\
         Handlers receive requests as `OwnedView<FooView<'static>>`, which gives\n\
         zero-copy borrowed access to fields (e.g. `request.name` is a `&str`\n\
         into the decoded buffer). The view can be held across `.await` points.\n\n\
         Implement methods with plain `async fn`; the returned future satisfies\n\
         the `Send` bound automatically. See the\n\
         [buffa user guide](https://github.com/anthropics/buffa/blob/main/docs/guide.md#ownedview-in-async-trait-implementations)\n\
         for zero-copy access patterns and when `to_owned_message()` is needed."
    );
    let service_doc_tokens = doc_attrs(&full_doc);

    // Generate trait methods
    let trait_methods: Vec<TokenStream> = service
        .method
        .iter()
        .map(|m| generate_trait_method(file, service, m, resolver, package))
        .collect::<Result<Vec<_>>>()?;

    // Generate route registrations for extension trait
    let route_registrations: Vec<TokenStream> = service
        .method
        .iter()
        .map(|m| {
            let method_name = m.name.as_deref().unwrap_or("");
            let method_snake = format_ident!("{}", method_name.to_snake_case());

            let client_streaming = m.client_streaming.unwrap_or(false);
            let server_streaming = m.server_streaming.unwrap_or(false);

            if server_streaming && !client_streaming {
                // Server streaming method
                quote! {
                    .route_view_server_stream(
                        #service_name_const,
                        #method_name,
                        view_streaming_handler_fn({
                            let svc = Arc::clone(&self);
                            move |ctx, req| {
                                let svc = Arc::clone(&svc);
                                async move { svc.#method_snake(ctx, req).await }
                            }
                        }),
                    )
                }
            } else if client_streaming && !server_streaming {
                // Client streaming method
                quote! {
                    .route_view_client_stream(
                        #service_name_const,
                        #method_name,
                        view_client_streaming_handler_fn({
                            let svc = Arc::clone(&self);
                            move |ctx, req| {
                                let svc = Arc::clone(&svc);
                                async move { svc.#method_snake(ctx, req).await }
                            }
                        }),
                    )
                }
            } else if client_streaming && server_streaming {
                // Bidi streaming method
                quote! {
                    .route_view_bidi_stream(
                        #service_name_const,
                        #method_name,
                        view_bidi_streaming_handler_fn({
                            let svc = Arc::clone(&self);
                            move |ctx, req| {
                                let svc = Arc::clone(&svc);
                                async move { svc.#method_snake(ctx, req).await }
                            }
                        }),
                    )
                }
            } else {
                // Unary method
                let is_idempotent = m
                    .options
                    .idempotency_level
                    .map(|level| level == IdempotencyLevel::NO_SIDE_EFFECTS)
                    .unwrap_or(false);

                let route_method = if is_idempotent {
                    quote! { route_view_idempotent }
                } else {
                    quote! { route_view }
                };

                quote! {
                    .#route_method(
                        #service_name_const,
                        #method_name,
                        {
                            let svc = Arc::clone(&self);
                            view_handler_fn(move |ctx, req| {
                                let svc = Arc::clone(&svc);
                                async move { svc.#method_snake(ctx, req).await }
                            })
                        },
                    )
                }
            }
        })
        .collect();

    // Generate client methods
    let client_methods: Vec<TokenStream> = service
        .method
        .iter()
        .map(|m| generate_client_method(&full_service_name, m, resolver, package))
        .collect::<Result<Vec<_>>>()?;

    // Generate monomorphic FooServiceServer<T> dispatcher.
    let service_server =
        generate_service_server(&full_service_name, &trait_name, service, resolver, package)?;

    // Example method name for client doc
    let example_method = service
        .method
        .first()
        .and_then(|m| m.name.as_deref())
        .map(|n| n.to_snake_case())
        .unwrap_or_else(|| "method".to_string());

    // Build client doc comment with interpolated example method
    let client_name_str = client_name.to_string();
    let client_doc = format!(
        r#"Client for this service.

Generic over `T: ClientTransport`. For **gRPC** (HTTP/2), use
`Http2Connection` — it has honest `poll_ready` and composes with
`tower::balance` for multi-connection load balancing. For **Connect
over HTTP/1.1** (or unknown protocol), use `HttpClient`.

# Example (gRPC / HTTP/2)

```rust,ignore
use connectrpc::client::{{Http2Connection, ClientConfig}};
use connectrpc::Protocol;

let uri: http::Uri = "http://localhost:8080".parse()?;
let conn = Http2Connection::connect_plaintext(uri.clone()).await?.shared(1024);
let config = ClientConfig::new(uri).protocol(Protocol::Grpc);

let client = {client_name_str}::new(conn, config);
let response = client.{example_method}(request).await?;
```

# Example (Connect / HTTP/1.1 or ALPN)

```rust,ignore
use connectrpc::client::{{HttpClient, ClientConfig}};

let http = HttpClient::plaintext();  // cleartext http:// only
let config = ClientConfig::new("http://localhost:8080".parse()?);

let client = {client_name_str}::new(http, config);
let response = client.{example_method}(request).await?;
```

# Working with the response

Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
The `OwnedView` derefs to the view, so field access is zero-copy:

```rust,ignore
let resp = client.{example_method}(request).await?.into_view();
let name: &str = resp.name;  // borrow into the response buffer
```

If you need the owned struct (e.g. to store or pass by value), use
[`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):

```rust,ignore
let owned = client.{example_method}(request).await?.into_owned();
```"#
    );
    let client_doc_tokens = doc_attrs(&client_doc);

    Ok(quote! {
        // -----------------------------------------------------------------------------
        // #service_name
        // -----------------------------------------------------------------------------

        /// Full service name for this service.
        pub const #service_name_const: &str = #full_service_name;

        #service_doc_tokens
        #[allow(clippy::type_complexity)]
        pub trait #trait_name: Send + Sync + 'static {
            #(#trait_methods)*
        }

        /// Extension trait for registering a service implementation with a Router.
        ///
        /// This trait is automatically implemented for all types that implement the service trait.
        ///
        /// # Example
        ///
        /// ```rust,ignore
        /// use std::sync::Arc;
        ///
        /// let service = Arc::new(MyServiceImpl);
        /// let router = service.register(Router::new());
        /// ```
        pub trait #ext_trait_name: #trait_name {
            /// Register this service implementation with a Router.
            ///
            /// Takes ownership of the `Arc<Self>` and returns a new Router with
            /// this service's methods registered.
            fn register(self: Arc<Self>, router: Router) -> Router;
        }

        impl<S: #trait_name> #ext_trait_name for S {
            fn register(self: Arc<Self>, router: Router) -> Router {
                router
                    #(#route_registrations)*
            }
        }

        #service_server

        #client_doc_tokens
        #[derive(Clone)]
        pub struct #client_name<T> {
            transport: T,
            config: ClientConfig,
        }

        impl<T> #client_name<T>
        where
            T: ClientTransport,
            <T::ResponseBody as http_body::Body>::Error: std::fmt::Display,
        {
            /// Create a new client with the given transport and configuration.
            pub fn new(transport: T, config: ClientConfig) -> Self {
                Self { transport, config }
            }

            /// Get the client configuration.
            pub fn config(&self) -> &ClientConfig {
                &self.config
            }

            /// Get a mutable reference to the client configuration.
            pub fn config_mut(&mut self) -> &mut ClientConfig {
                &mut self.config
            }

            #(#client_methods)*
        }
    })
}

/// Generate a monomorphic `FooServiceServer<T>` struct and its `Dispatcher` impl.
///
/// This is the fast-path alternative to `FooServiceExt::register(Router)`: instead
/// of type-erasing each method behind `Arc<dyn ErasedHandler>` and looking them up
/// in a `HashMap`, this struct dispatches via a compile-time `match` on method name
/// with no trait objects or hash lookups in the hot path.
fn generate_service_server(
    full_service_name: &str,
    trait_name: &proc_macro2::Ident,
    service: &ServiceDescriptorProto,
    resolver: &TypeResolver<'_>,
    package: &str,
) -> Result<TokenStream> {
    let server_name = format_ident!("{}Server", trait_name);
    // Path prefix matched by `dispatch` / `call_*`: "pkg.Service/"
    let path_prefix = format!("{full_service_name}/");

    // Per-method match arms for `lookup(path)`.
    let lookup_arms: Vec<TokenStream> = service
        .method
        .iter()
        .map(|m| {
            let method_name = m.name.as_deref().unwrap_or("");
            let client_streaming = m.client_streaming.unwrap_or(false);
            let server_streaming = m.server_streaming.unwrap_or(false);
            let is_idempotent = m
                .options
                .idempotency_level
                .map(|level| level == IdempotencyLevel::NO_SIDE_EFFECTS)
                .unwrap_or(false);

            let desc = if client_streaming && server_streaming {
                quote! { __crpc_codegen::MethodDescriptor::bidi_streaming() }
            } else if client_streaming {
                quote! { __crpc_codegen::MethodDescriptor::client_streaming() }
            } else if server_streaming {
                quote! { __crpc_codegen::MethodDescriptor::server_streaming() }
            } else {
                quote! { __crpc_codegen::MethodDescriptor::unary(#is_idempotent) }
            };
            quote! { #method_name => Some(#desc), }
        })
        .collect();

    // Per-kind match arms for the four `call_*` methods.
    // Each `call_*` only includes arms for methods of the matching kind; other
    // paths fall through to `unimplemented_*` (the caller checked `lookup()`
    // first, so this is a defensive-only branch).
    let mut call_unary_arms: Vec<TokenStream> = Vec::new();
    let mut call_ss_arms: Vec<TokenStream> = Vec::new();
    let mut call_cs_arms: Vec<TokenStream> = Vec::new();
    let mut call_bidi_arms: Vec<TokenStream> = Vec::new();

    for m in &service.method {
        let method_name = m.name.as_deref().unwrap_or("");
        let method_snake = format_ident!("{}", method_name.to_snake_case());
        let input_view = resolver.rust_view_type(m.input_type.as_deref().unwrap_or(""), package);
        let cs = m.client_streaming.unwrap_or(false);
        let ss = m.server_streaming.unwrap_or(false);

        if cs && ss {
            // Bidi streaming
            call_bidi_arms.push(quote! {
                #method_name => {
                    let svc = Arc::clone(&self.inner);
                    Box::pin(async move {
                        let req_stream = __crpc_codegen::decode_view_request_stream::<#input_view>(requests, format);
                        let (resp_stream, ctx) = svc.#method_snake(ctx, req_stream).await?;
                        Ok((__crpc_codegen::encode_response_stream(resp_stream, format), ctx))
                    })
                }
            });
        } else if cs {
            // Client streaming
            call_cs_arms.push(quote! {
                #method_name => {
                    let svc = Arc::clone(&self.inner);
                    Box::pin(async move {
                        let req_stream = __crpc_codegen::decode_view_request_stream::<#input_view>(requests, format);
                        let (res, ctx) = svc.#method_snake(ctx, req_stream).await?;
                        let bytes = __crpc_codegen::encode_response(&res, format)?;
                        Ok((bytes, ctx))
                    })
                }
            });
        } else if ss {
            // Server streaming
            call_ss_arms.push(quote! {
                #method_name => {
                    let svc = Arc::clone(&self.inner);
                    Box::pin(async move {
                        let req = __crpc_codegen::decode_request_view::<#input_view>(request, format)?;
                        let (resp_stream, ctx) = svc.#method_snake(ctx, req).await?;
                        Ok((__crpc_codegen::encode_response_stream(resp_stream, format), ctx))
                    })
                }
            });
        } else {
            // Unary
            call_unary_arms.push(quote! {
                #method_name => {
                    let svc = Arc::clone(&self.inner);
                    Box::pin(async move {
                        let req = __crpc_codegen::decode_request_view::<#input_view>(request, format)?;
                        let (res, ctx) = svc.#method_snake(ctx, req).await?;
                        let bytes = __crpc_codegen::encode_response(&res, format)?;
                        Ok((bytes, ctx))
                    })
                }
            });
        }
    }

    let server_doc = format!(
        "Monomorphic dispatcher for `{trait_name}`.\n\n\
         Unlike `.register(Router)` which type-erases each method into an \
         `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches \
         via a compile-time `match` on method name: no vtable, no hash lookup.\n\n\
         # Example\n\n\
         ```rust,ignore\n\
         use connectrpc::ConnectRpcService;\n\n\
         let server = {server_name}::new(MyImpl);\n\
         let service = ConnectRpcService::new(server);\n\
         // hand `service` to axum/hyper as a fallback_service\n\
         ```"
    );
    let server_doc_tokens = doc_attrs(&server_doc);

    Ok(quote! {
        #server_doc_tokens
        pub struct #server_name<T> {
            inner: Arc<T>,
        }

        impl<T: #trait_name> #server_name<T> {
            /// Wrap a service implementation in a monomorphic dispatcher.
            pub fn new(service: T) -> Self {
                Self { inner: Arc::new(service) }
            }

            /// Wrap an already-`Arc`'d service implementation.
            pub fn from_arc(inner: Arc<T>) -> Self {
                Self { inner }
            }
        }

        impl<T> Clone for #server_name<T> {
            fn clone(&self) -> Self {
                Self { inner: Arc::clone(&self.inner) }
            }
        }

        impl<T: #trait_name> Dispatcher for #server_name<T> {
            #[inline]
            fn lookup(&self, path: &str) -> Option<__crpc_codegen::MethodDescriptor> {
                let method = path.strip_prefix(#path_prefix)?;
                match method {
                    #(#lookup_arms)*
                    _ => None,
                }
            }

            fn call_unary(
                &self,
                path: &str,
                ctx: Context,
                request: __Bytes,
                format: __CodecFormat,
            ) -> __crpc_codegen::UnaryResult {
                let Some(method) = path.strip_prefix(#path_prefix) else {
                    return __crpc_codegen::unimplemented_unary(path);
                };
                // Suppress unused warnings when this service has no unary methods.
                let _ = (&ctx, &request, &format);
                match method {
                    #(#call_unary_arms)*
                    _ => __crpc_codegen::unimplemented_unary(path),
                }
            }

            fn call_server_streaming(
                &self,
                path: &str,
                ctx: Context,
                request: __Bytes,
                format: __CodecFormat,
            ) -> __crpc_codegen::StreamingResult {
                let Some(method) = path.strip_prefix(#path_prefix) else {
                    return __crpc_codegen::unimplemented_streaming(path);
                };
                let _ = (&ctx, &request, &format);
                match method {
                    #(#call_ss_arms)*
                    _ => __crpc_codegen::unimplemented_streaming(path),
                }
            }

            fn call_client_streaming(
                &self,
                path: &str,
                ctx: Context,
                requests: __crpc_codegen::RequestStream,
                format: __CodecFormat,
            ) -> __crpc_codegen::UnaryResult {
                let Some(method) = path.strip_prefix(#path_prefix) else {
                    return __crpc_codegen::unimplemented_unary(path);
                };
                let _ = (&ctx, &requests, &format);
                match method {
                    #(#call_cs_arms)*
                    _ => __crpc_codegen::unimplemented_unary(path),
                }
            }

            fn call_bidi_streaming(
                &self,
                path: &str,
                ctx: Context,
                requests: __crpc_codegen::RequestStream,
                format: __CodecFormat,
            ) -> __crpc_codegen::StreamingResult {
                let Some(method) = path.strip_prefix(#path_prefix) else {
                    return __crpc_codegen::unimplemented_streaming(path);
                };
                let _ = (&ctx, &requests, &format);
                match method {
                    #(#call_bidi_arms)*
                    _ => __crpc_codegen::unimplemented_streaming(path),
                }
            }
        }
    })
}

/// Generate documentation comment tokens.
fn generate_doc_comment(doc: &str, default: &str) -> TokenStream {
    let comment = if doc.is_empty() { default } else { doc };
    doc_attrs(comment)
}

/// Generate a trait method for a service.
fn generate_trait_method(
    file: &FileDescriptorProto,
    service: &ServiceDescriptorProto,
    method: &MethodDescriptorProto,
    resolver: &TypeResolver<'_>,
    package: &str,
) -> Result<TokenStream> {
    let method_name = method.name.as_deref().unwrap_or("");
    let method_snake = format_ident!("{}", method_name.to_snake_case());
    let input_view_type =
        resolver.rust_view_type(method.input_type.as_deref().unwrap_or(""), package);
    let output_type = resolver.rust_type(method.output_type.as_deref().unwrap_or(""), package);

    // Get method documentation
    let method_doc = get_method_comment(file, service, method).unwrap_or_default();
    let method_doc_tokens =
        generate_doc_comment(&method_doc, &format!("Handle the {method_name} RPC."));

    // Check for streaming
    let client_streaming = method.client_streaming.unwrap_or(false);
    let server_streaming = method.server_streaming.unwrap_or(false);

    if server_streaming && !client_streaming {
        // Server streaming method
        Ok(quote! {
            #method_doc_tokens
            fn #method_snake(
                &self,
                ctx: Context,
                request: OwnedView<#input_view_type<'static>>,
            ) -> impl Future<Output = Result<(Pin<Box<dyn Stream<Item = Result<#output_type, ConnectError>> + Send>>, Context), ConnectError>> + Send;
        })
    } else if client_streaming && !server_streaming {
        // Client streaming method
        Ok(quote! {
            #method_doc_tokens
            fn #method_snake(
                &self,
                ctx: Context,
                requests: Pin<Box<dyn Stream<Item = Result<OwnedView<#input_view_type<'static>>, ConnectError>> + Send>>,
            ) -> impl Future<Output = Result<(#output_type, Context), ConnectError>> + Send;
        })
    } else if client_streaming && server_streaming {
        // Bidi streaming method
        Ok(quote! {
            #method_doc_tokens
            fn #method_snake(
                &self,
                ctx: Context,
                requests: Pin<Box<dyn Stream<Item = Result<OwnedView<#input_view_type<'static>>, ConnectError>> + Send>>,
            ) -> impl Future<Output = Result<(Pin<Box<dyn Stream<Item = Result<#output_type, ConnectError>> + Send>>, Context), ConnectError>> + Send;
        })
    } else {
        // Unary method
        Ok(quote! {
            #method_doc_tokens
            fn #method_snake(
                &self,
                ctx: Context,
                request: OwnedView<#input_view_type<'static>>,
            ) -> impl Future<Output = Result<(#output_type, Context), ConnectError>> + Send;
        })
    }
}

/// Generate client method(s) for a service RPC.
///
/// Emits two methods per RPC:
///   - `<method_snake>(&self, ...)` — no-options convenience, delegates to `_with_options`
///   - `<method_snake>_with_options(&self, ..., options: CallOptions)` — explicit options
///
/// This gives callers an ergonomic default while still surfacing per-call
/// control. The library's `effective_options()` merges options over
/// ClientConfig defaults, so the no-options variant still picks up any
/// client-wide defaults the user configured.
fn generate_client_method(
    full_service_name: &str,
    method: &MethodDescriptorProto,
    resolver: &TypeResolver<'_>,
    package: &str,
) -> Result<TokenStream> {
    let method_name = method.name.as_deref().unwrap_or("");
    let method_snake = format_ident!("{}", method_name.to_snake_case());
    let method_with_opts = format_ident!("{}_with_options", method_name.to_snake_case());
    let input_type = resolver.rust_type(method.input_type.as_deref().unwrap_or(""), package);
    let output_view_type =
        resolver.rust_view_type(method.output_type.as_deref().unwrap_or(""), package);

    let client_streaming = method.client_streaming.unwrap_or(false);
    let server_streaming = method.server_streaming.unwrap_or(false);

    let doc = format!(
        " Call the {method_name} RPC. Sends a request to /{full_service_name}/{method_name}."
    );
    let doc_opts = format!(
        " Call the {method_name} RPC with explicit per-call options. \
         Options override [`ClientConfig`] defaults."
    );

    // Return type is protocol-specific. Compute once.
    let ret_ty: TokenStream;
    let call_body: TokenStream;
    let short_args: TokenStream; // args to the no-opts convenience method
    let opts_args: TokenStream; // args to the _with_options method
    let short_delegate_args: TokenStream; // how short delegates to opts

    if client_streaming && !server_streaming {
        // Client-stream
        ret_ty = quote! {
            Result<
                ::connectrpc::client::UnaryResponse<OwnedView<#output_view_type<'static>>>,
                ConnectError,
            >
        };
        call_body = quote! {
            call_client_stream(
                &self.transport, &self.config,
                #full_service_name, #method_name,
                requests, options,
            ).await
        };
        short_args = quote! { requests: impl IntoIterator<Item = #input_type> };
        opts_args =
            quote! { requests: impl IntoIterator<Item = #input_type>, options: CallOptions };
        short_delegate_args = quote! { requests, CallOptions::default() };
    } else if client_streaming && server_streaming {
        // Bidi
        ret_ty = quote! {
            Result<
                ::connectrpc::client::BidiStream<
                    T::ResponseBody, #input_type, #output_view_type<'static>
                >,
                ConnectError,
            >
        };
        call_body = quote! {
            call_bidi_stream(
                &self.transport, &self.config,
                #full_service_name, #method_name, options,
            ).await
        };
        short_args = quote! {};
        opts_args = quote! { options: CallOptions };
        short_delegate_args = quote! { CallOptions::default() };
    } else if server_streaming {
        // Server-stream
        ret_ty = quote! {
            Result<
                ::connectrpc::client::ServerStream<T::ResponseBody, #output_view_type<'static>>,
                ConnectError,
            >
        };
        call_body = quote! {
            call_server_stream(
                &self.transport, &self.config,
                #full_service_name, #method_name,
                request, options,
            ).await
        };
        short_args = quote! { request: #input_type };
        opts_args = quote! { request: #input_type, options: CallOptions };
        short_delegate_args = quote! { request, CallOptions::default() };
    } else {
        // Unary
        ret_ty = quote! {
            Result<
                ::connectrpc::client::UnaryResponse<OwnedView<#output_view_type<'static>>>,
                ConnectError,
            >
        };
        call_body = quote! {
            call_unary(
                &self.transport, &self.config,
                #full_service_name, #method_name,
                request, options,
            ).await
        };
        short_args = quote! { request: #input_type };
        opts_args = quote! { request: #input_type, options: CallOptions };
        short_delegate_args = quote! { request, CallOptions::default() };
    }

    Ok(quote! {
        #[doc = #doc]
        pub async fn #method_snake(&self, #short_args) -> #ret_ty {
            self.#method_with_opts(#short_delegate_args).await
        }

        #[doc = #doc_opts]
        pub async fn #method_with_opts(&self, #opts_args) -> #ret_ty {
            #call_body
        }
    })
}

/// Get the documentation comment for a service.
fn get_service_comment(
    file: &FileDescriptorProto,
    service: &ServiceDescriptorProto,
) -> Option<String> {
    // MessageField derefs to default when unset; default has empty location vec
    let source_info: &SourceCodeInfo = &file.source_code_info;

    // Find service index
    let service_index = file.service.iter().position(|s| s.name == service.name)?;

    // Path for service: [6, service_index]
    // 6 = service field number in FileDescriptorProto
    let target_path = vec![6, service_index as i32];

    find_comment(source_info, &target_path)
}

/// Get the documentation comment for a method.
fn get_method_comment(
    file: &FileDescriptorProto,
    service: &ServiceDescriptorProto,
    method: &MethodDescriptorProto,
) -> Option<String> {
    let source_info: &SourceCodeInfo = &file.source_code_info;

    // Find service and method indices, matching on the parent service name
    // to avoid ambiguity when multiple services have methods with the same name.
    let (service_index, method_index) = file.service.iter().enumerate().find_map(|(si, s)| {
        if s.name != service.name {
            return None;
        }
        s.method
            .iter()
            .position(|m| m.name == method.name)
            .map(|mi| (si, mi))
    })?;

    // Path for method: [6, service_index, 2, method_index]
    // 6 = service field number in FileDescriptorProto
    // 2 = method field number in ServiceDescriptorProto
    let target_path = vec![6, service_index as i32, 2, method_index as i32];

    find_comment(source_info, &target_path)
}

/// Find a comment in source code info for the given path.
fn find_comment(source_info: &SourceCodeInfo, target_path: &[i32]) -> Option<String> {
    for location in &source_info.location {
        if location.path == target_path {
            let comment = location
                .leading_comments
                .as_ref()
                .or(location.trailing_comments.as_ref())?;

            // Trim each line; blank lines are dropped (protoc's convention
            // uses a leading space we don't need here — `doc_attrs` adds
            // its own uniform leading space for prettyplease rendering).
            let cleaned: String = comment
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n");

            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffa_codegen::generated::descriptor::DescriptorProto;

    #[test]
    fn doc_attrs_prefixes_space_for_prettyplease() {
        // prettyplease emits `#[doc = "X"]` as `///X` verbatim. We prefix
        // each non-blank line with a space so the output is `/// X`.
        let ts = quote! {
            #[allow(dead_code)]
            mod m {}
        };
        let doc = doc_attrs("Hello.\n\nSecond paragraph.");
        let combined = quote! { #doc #ts };
        let file = syn::parse2::<syn::File>(combined).unwrap();
        let out = prettyplease::unparse(&file);
        // Each non-blank line should have a space after ///.
        assert!(out.contains("/// Hello."), "got: {out}");
        assert!(out.contains("/// Second paragraph."), "got: {out}");
        // Blank line becomes bare /// (paragraph break).
        assert!(out.contains("///\n"), "got: {out}");
        // Should NOT contain ///H (no space) or ///  H (double space).
        assert!(!out.contains("///Hello"), "got: {out}");
        assert!(!out.contains("///  Hello"), "got: {out}");
    }

    /// Build a minimal proto file with one message type and one service method.
    /// The service method's input/output types are fully-qualified proto names
    /// (e.g. `.example.v1.PingReq` or `.google.protobuf.Empty`) so the resolver
    /// can look them up.
    fn minimal_file(
        package: Option<&str>,
        input_type: &str,
        output_type: &str,
        local_messages: &[&str],
    ) -> FileDescriptorProto {
        let method = MethodDescriptorProto {
            name: Some("Ping".into()),
            input_type: Some(input_type.into()),
            output_type: Some(output_type.into()),
            ..Default::default()
        };
        let service = ServiceDescriptorProto {
            name: Some("PingService".into()),
            method: vec![method],
            ..Default::default()
        };
        FileDescriptorProto {
            name: Some("ping.proto".into()),
            package: package.map(|p| p.into()),
            service: vec![service],
            message_type: local_messages
                .iter()
                .map(|name| DescriptorProto {
                    name: Some((*name).into()),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Generate service code for `files[target_idx]`. All files are visible
    /// to the resolver (as transitive deps via `--include_imports`), but
    /// only the target is in `file_to_generate` — mirroring real protoc use.
    fn gen_service(files: &[FileDescriptorProto], target_idx: usize) -> String {
        let config = buffa_codegen::CodeGenConfig::default();
        let target_name = files[target_idx]
            .name
            .clone()
            .into_iter()
            .collect::<Vec<_>>();
        let resolver = TypeResolver::new(files, &target_name, &config);
        let file = &files[target_idx];
        let service = &file.service[0];
        generate_service(file, service, &resolver)
            .unwrap()
            .to_string()
    }

    #[test]
    fn service_name_with_package() {
        let file = minimal_file(
            Some("example.v1"),
            ".example.v1.PingReq",
            ".example.v1.PingResp",
            &["PingReq", "PingResp"],
        );
        let code = gen_service(std::slice::from_ref(&file), 0);
        assert!(code.contains("\"example.v1.PingService\""), "got: {code}");
    }

    #[test]
    fn service_name_without_package() {
        // Empty package must produce "PingService", not ".PingService".
        let file = minimal_file(None, ".PingReq", ".PingResp", &["PingReq", "PingResp"]);
        let code = gen_service(std::slice::from_ref(&file), 0);
        assert!(code.contains("\"PingService\""), "got: {code}");
        assert!(
            !code.contains("\".PingService\""),
            "must not have leading dot: {code}"
        );
    }

    #[test]
    fn same_package_types_use_bare_names() {
        let file = minimal_file(
            Some("example.v1"),
            ".example.v1.PingReq",
            ".example.v1.PingResp",
            &["PingReq", "PingResp"],
        );
        let code = gen_service(std::slice::from_ref(&file), 0);
        // Same-package types resolve to bare identifiers.
        assert!(code.contains("PingReq"), "input type missing: {code}");
        assert!(code.contains("PingResp"), "output type missing: {code}");
        // No super:: prefix for same-package types.
        assert!(
            !code.contains("super :: PingReq"),
            "unexpected super: {code}"
        );
    }

    #[test]
    fn cross_package_types_use_relative_paths() {
        // Service in example.v1 references types from common.v1.
        // Must emit a super::-relative path matching buffa's module
        // layout, not bare `Shared` (which would fail to compile).
        let common = FileDescriptorProto {
            name: Some("common.proto".into()),
            package: Some("common.v1".into()),
            message_type: vec![DescriptorProto {
                name: Some("Shared".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let svc = minimal_file(
            Some("example.v1"),
            ".common.v1.Shared",
            ".example.v1.Out",
            &["Out"],
        );
        let code = gen_service(&[common, svc], 1);

        // example.v1 -> super::super -> common::v1::Shared
        // (token stream stringifies `::` with spaces, so match loosely)
        assert!(
            code.contains("super :: super :: common :: v1 :: Shared"),
            "cross-package path not emitted: {code}"
        );
        assert!(
            code.contains("super :: super :: common :: v1 :: SharedView"),
            "cross-package view path not emitted: {code}"
        );
    }

    #[test]
    fn wkt_types_use_buffa_types_extern_path() {
        // Service referencing google.protobuf.Empty as an input/output
        // type. WKT auto-injection maps it to ::buffa_types::..., same
        // path buffa-codegen emits for WKT message fields.
        let wkt = FileDescriptorProto {
            name: Some("google/protobuf/empty.proto".into()),
            package: Some("google.protobuf".into()),
            message_type: vec![DescriptorProto {
                name: Some("Empty".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let svc = minimal_file(
            Some("example.v1"),
            ".google.protobuf.Empty",
            ".example.v1.Out",
            &["Out"],
        );
        let code = gen_service(&[wkt, svc], 1);

        assert!(
            code.contains(":: buffa_types :: google :: protobuf :: Empty"),
            "WKT extern path not emitted: {code}"
        );
    }
}
