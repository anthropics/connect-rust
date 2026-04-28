//! Code generation logic for ConnectRPC Rust bindings.
//!
//! This module generates:
//! - Buffa message types (via buffa-codegen)
//! - ConnectRPC service traits and clients
//!
//! Code generation uses the `quote` crate for producing Rust code from
//! TokenStreams, which provides better syntax highlighting, type safety,
//! and maintainability compared to string-based generation.

use std::collections::HashMap;

use anyhow::Result;
use heck::ToSnakeCase;
use heck::ToUpperCamelCase;
use proc_macro2::{Ident, TokenStream};
use quote::format_ident;
use quote::quote;

use buffa_codegen::generated::descriptor::FileDescriptorProto;
use buffa_codegen::generated::descriptor::MethodDescriptorProto;
use buffa_codegen::generated::descriptor::ServiceDescriptorProto;
use buffa_codegen::generated::descriptor::SourceCodeInfo;
use buffa_codegen::generated::descriptor::method_options::IdempotencyLevel;
use buffa_codegen::idents::make_field_ident;
use buffa_codegen::idents::rust_path_to_tokens;

pub use buffa_codegen::generated::descriptor;
pub use buffa_codegen::{CodeGenConfig, GeneratedFile, GeneratedFileKind};

use crate::plugin::CodeGeneratorRequest;
use crate::plugin::CodeGeneratorResponse;
use crate::plugin::CodeGeneratorResponseFile;

/// Options for ConnectRPC code generation.
///
/// These control both the underlying buffa message generation and the
/// ConnectRPC service binding generation.
///
/// Construct via `Options::default()` then set fields on `buffa` directly
/// (the struct is `#[non_exhaustive]`, so struct-update syntax is
/// unavailable from outside this crate).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Options {
    /// The underlying buffa-codegen configuration. Set any
    /// [`CodeGenConfig`] field directly here; connectrpc passes it through
    /// verbatim except for [`CodeGenConfig::generate_views`], which is
    /// forced to `true` (service stubs require view types).
    ///
    /// [`Options::default()`] starts from buffa's defaults but enables
    /// `generate_json` (the Connect protocol's JSON codec needs it; buffa's
    /// own default is `false`).
    ///
    /// `buffa.extern_paths` is used by [`generate_services`] to bake
    /// absolute paths into service stubs (set a `(".", "crate::proto")`
    /// catch-all so every type resolves); it is ignored by
    /// [`generate_files`] (the unified `super::`-relative path).
    pub buffa: CodeGenConfig,
}

impl Default for Options {
    fn default() -> Self {
        let mut buffa = CodeGenConfig::default();
        buffa.generate_json = true;
        Self { buffa }
    }
}

impl Options {
    /// Clone the embedded buffa config and apply connectrpc's invariants
    /// (`generate_views = true` — service stubs reference view types).
    fn to_buffa_config(&self) -> CodeGenConfig {
        let mut config = self.buffa.clone();
        config.generate_views = true;
        config
    }
}

/// Emit one [`GeneratedFile`] per proto file in `file_to_generate` that
/// declares at least one `service`. Files with no services produce no output.
fn emit_service_files(
    proto_file: &[FileDescriptorProto],
    file_to_generate: &[String],
    resolver: &TypeResolver<'_>,
) -> Result<Vec<GeneratedFile>> {
    use std::collections::BTreeSet;
    let mut out = Vec::new();
    // Dedup output-type Encodable impls across the whole batch, not per
    // file: two files in the same package whose RPCs share an output
    // type would otherwise both emit `impl Encodable<Out> for OutView`
    // and collide with E0119 once stitched into one module.
    let mut encodable_seen: BTreeSet<String> = BTreeSet::new();
    for file_name in file_to_generate {
        let file_desc = proto_file
            .iter()
            .find(|f| f.name.as_deref() == Some(file_name.as_str()));

        if let Some(file) = file_desc
            && !file.service.is_empty()
        {
            let service_tokens = generate_connect_services(file, resolver, &mut encodable_seen)?;
            let service_code = format_token_stream(&service_tokens)?;
            // In the unified path the service code is appended to buffa's
            // `<stem>.rs` (Owned) file by [`generate_files`], so name and
            // kind match that file. In the split path this stands alone but
            // is still wired as a content file.
            out.push(GeneratedFile {
                name: format!("{}.rs", buffa_codegen::proto_path_to_stem(file_name)),
                package: file.package.clone().unwrap_or_default(),
                kind: GeneratedFileKind::Owned,
                content: service_code,
            });
        }
    }
    Ok(out)
}

/// Generate ConnectRPC service bindings + buffa message types from proto
/// descriptors.
///
/// Returns buffa's six [`GeneratedFile`]s per proto (Owned, View, Oneof,
/// ViewOneof, Ext, plus one PackageMod stitcher per package), with service
/// stubs appended to each `<stem>.rs` Owned content file. Callers write
/// every file to disk and wire only the [`GeneratedFileKind::PackageMod`]
/// entries into their module tree (the stitchers `include!` the rest).
///
/// This is the **unified** path: service stubs reference message types via
/// `super::`-relative paths, so both must live in the same module tree.
/// [`CodeGenConfig::extern_paths`] is ignored.
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

    let mut files = buffa_codegen::generate(proto_file, file_to_generate, &config)
        .map_err(|e| anyhow::anyhow!("buffa-codegen failed: {e}"))?;

    let resolver = TypeResolver::new(proto_file, file_to_generate, &config, false);
    let service_files = emit_service_files(proto_file, file_to_generate, &resolver)?;

    // Append each service file's content to the matching message file.
    for svc in service_files {
        if let Some(out) = files.iter_mut().find(|g| g.name == svc.name) {
            out.content.push('\n');
            out.content.push_str(&svc.content);
        }
    }

    Ok(files)
}

/// Generate **only** ConnectRPC service bindings from proto descriptors.
///
/// Returns one `GeneratedFile` per proto file in `file_to_generate` that
/// declares at least one `service`. No message types, no `mod.rs`.
///
/// This is the **split** path: service stubs reference message types via
/// absolute Rust paths derived from [`CodeGenConfig::extern_paths`]. Callers must
/// set at least a `.` catch-all entry (e.g. `(".", "crate::proto")`) so
/// every type resolves; the auto-injected WKT mapping still takes priority
/// via longest-prefix-match. The generated code compiles standalone as long
/// as the extern paths point at a buffa-generated module tree.
///
/// # Errors
///
/// Errors if any method input/output type is not covered by an extern_path
/// mapping, or is absent from `proto_file` (missing import).
pub fn generate_services(
    proto_file: &[FileDescriptorProto],
    file_to_generate: &[String],
    options: &Options,
) -> Result<Vec<GeneratedFile>> {
    use std::collections::BTreeMap;

    let config = options.to_buffa_config();
    let resolver = TypeResolver::new(proto_file, file_to_generate, &config, true);
    let mut files = emit_service_files(proto_file, file_to_generate, &resolver)?;

    // Emit a per-package `<pkg>.mod.rs` stitcher for each package with at
    // least one service-declaring proto, so `protoc-gen-buffa-packaging`
    // can wire this output the same way it wires buffa's. The stitcher
    // here is trivial — just `include!("<stem>.rs")` per file; there's no
    // `__buffa::` ancillary tree for service stubs.
    let mut by_package: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in &files {
        by_package
            .entry(f.package.clone())
            .or_default()
            .push(f.name.clone());
    }
    for (package, names) in by_package {
        let mut content = String::from("// @generated by connectrpc-codegen. DO NOT EDIT.\n");
        for n in &names {
            // {:?} on the filename gives a quoted, escaped string literal.
            content.push_str(&format!("include!({n:?});\n"));
        }
        files.push(GeneratedFile {
            name: buffa_codegen::package_to_mod_filename(&package),
            package,
            kind: GeneratedFileKind::PackageMod,
            content,
        });
    }

    Ok(files)
}

/// Generate a `CodeGeneratorResponse` from a protoc `CodeGeneratorRequest`.
///
/// This is the entry point for the protoc plugin (`protoc-gen-connect-rust`).
/// It parses the comma-separated `request.parameter` into [`Options`] and
/// delegates to [`generate_services`] — service stubs only. Callers must
/// run `protoc-gen-buffa` (or equivalent) separately for message types.
///
/// # Output
///
/// Per proto with at least one `service`: a `<stem>.rs` content file with
/// the service stubs. Per package with at least one such proto: a
/// `<pkg>.mod.rs` stitcher that `include!`s the content files. The
/// stitcher filename intentionally matches `protoc-gen-buffa`'s, so run
/// this plugin into a separate output directory and use
/// `protoc-gen-buffa-packaging` to wire both trees, as shown in this
/// repo's `buf.gen.yaml` examples.
///
/// # Recognized options
///
/// - `buffa_module=<rust_path>` — where you mounted the buffa-generated
///   module tree (e.g. `buffa_module=crate::proto`). Shorthand for
///   `extern_path=.=<rust_path>`. This is the option most local users want.
/// - `extern_path=<proto>=<rust>` — map a specific proto package prefix
///   to a Rust module path. Repeatable; longest-prefix-match wins.
///   `extern_path=.=<path>` is the catch-all (equivalent to `buffa_module`).
///   At least one catch-all mapping is required so every type resolves.
/// - `strict_utf8_mapping` — see [`CodeGenConfig::strict_utf8_mapping`].
/// - `no_json` — disable `serde` derives on generated message types.
///   Ignored in this plugin (no message types emitted); accepted for
///   compatibility with the unified path.
/// - `no_register_fn` — suppress the per-file
///   `register_types(&mut TypeRegistry)` aggregator. See
///   [`CodeGenConfig::emit_register_fn`]. Ignored in this plugin (no message
///   types emitted); accepted for compatibility with the unified path.
pub fn generate(request: &CodeGeneratorRequest) -> Result<CodeGeneratorResponse> {
    let mut options = Options::default();

    if let Some(ref param) = request.parameter {
        for opt in param.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(value) = opt.strip_prefix("buffa_module=") {
                let rust = value.trim();
                if rust.is_empty() {
                    anyhow::bail!(
                        "buffa_module requires a non-empty path, \
                         e.g. buffa_module=crate::proto"
                    );
                }
                options
                    .buffa
                    .extern_paths
                    .push((".".into(), rust.to_string()));
            } else if let Some(value) = opt.strip_prefix("extern_path=") {
                // value is "<proto_path>=<rust_path>"
                let (proto, rust) = value.split_once('=').ok_or_else(|| {
                    anyhow::anyhow!(
                        "invalid extern_path format {value:?}, expected \
                         extern_path=.proto.pkg=::rust::path"
                    )
                })?;
                let proto = proto.trim();
                let rust = rust.trim();
                if proto.is_empty() || rust.is_empty() {
                    anyhow::bail!(
                        "invalid extern_path format {value:?}, expected \
                         extern_path=.proto.pkg=::rust::path (both sides non-empty)"
                    );
                }
                let mut proto = proto.to_string();
                if !proto.starts_with('.') {
                    proto.insert(0, '.');
                }
                options.buffa.extern_paths.push((proto, rust.to_string()));
            } else {
                match opt {
                    "strict_utf8_mapping" => options.buffa.strict_utf8_mapping = true,
                    "no_json" => options.buffa.generate_json = false,
                    "no_register_fn" => options.buffa.emit_register_fn = false,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "unknown plugin option: {opt:?}. Supported: \
                             buffa_module=<rust_path>, extern_path=<proto>=<rust>, \
                             strict_utf8_mapping, no_json, no_register_fn"
                        ));
                    }
                }
            }
        }
    }

    let generated = generate_services(&request.proto_file, &request.file_to_generate, &options)?;

    let files: Vec<CodeGeneratorResponseFile> = generated
        .into_iter()
        .map(|g| CodeGeneratorResponseFile {
            name: Some(g.name),
            content: Some(g.content),
            ..Default::default()
        })
        .collect();

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
    /// When true, every resolved path must be absolute (`::foo` or
    /// `crate::foo`). Paths that would resolve to `super::`-relative or
    /// bare-ident forms produce an error instead. Used by
    /// [`generate_services`] to enforce that service stubs reference
    /// message types via `extern_path` only.
    require_extern: bool,
}

impl<'a> TypeResolver<'a> {
    fn new(
        proto_file: &'a [FileDescriptorProto],
        file_to_generate: &[String],
        config: &'a buffa_codegen::CodeGenConfig,
        require_extern: bool,
    ) -> Self {
        Self {
            ctx: buffa_codegen::context::CodeGenContext::for_generate(
                proto_file,
                file_to_generate,
                config,
            ),
            require_extern,
        }
    }

    /// Resolve a proto FQN (e.g. `.google.protobuf.Empty`) to a Rust type-path
    /// string relative to `current_package`.
    ///
    /// In `require_extern` mode, errors if the path is not absolute or the
    /// type is absent from the descriptor set. Otherwise falls back to the
    /// bare type name for unknown types (rustc will point at the use site).
    fn resolve_path(&self, proto_fqn: &str, current_package: &str) -> Result<String> {
        match self.ctx.rust_type_relative(proto_fqn, current_package, 0) {
            Some(path) => {
                self.check_extern_coverage(proto_fqn, &path)?;
                Ok(path)
            }
            None => self.fallback_unresolved(proto_fqn).map(str::to_string),
        }
    }

    /// In `require_extern` mode, fail if `path_prefix` isn't an absolute or
    /// crate-rooted path (i.e., the type wasn't covered by an extern_path
    /// mapping). No-op otherwise.
    fn check_extern_coverage(&self, proto_fqn: &str, path_prefix: &str) -> Result<()> {
        if self.require_extern
            && !path_prefix.starts_with("::")
            && !path_prefix.starts_with("crate::")
        {
            anyhow::bail!(
                "type {proto_fqn} is not covered by any extern_path mapping. \
                 Add extern_path=.=<your_buffa_module> (e.g. \
                 extern_path=.=crate::proto) to the plugin opts."
            );
        }
        Ok(())
    }

    /// Fallback when a FQN is absent from the descriptor set: error in
    /// `require_extern` mode, otherwise return the bare type name (rustc
    /// will point at the use site if it's wrong).
    fn fallback_unresolved<'f>(&self, proto_fqn: &'f str) -> Result<&'f str> {
        if self.require_extern {
            anyhow::bail!("type {proto_fqn} not found in descriptor set (missing proto import?)");
        }
        Ok(bare_type_name(proto_fqn))
    }

    /// Resolve a proto FQN to Rust type-path tokens.
    fn rust_type(&self, proto_fqn: &str, current_package: &str) -> Result<TokenStream> {
        let path = self.resolve_path(proto_fqn, current_package)?;
        Ok(rust_path_to_tokens(&path))
    }

    /// Resolve a proto FQN to its **view** Rust type-path tokens.
    ///
    /// Under buffa's `__buffa::` ancillary tree, view types live at
    /// `<to-package>::__buffa::view::<within-package>View`, so this uses
    /// `CodeGenContext::rust_type_relative_split` to find the package
    /// boundary and inserts the sentinel path between the two halves.
    fn rust_view_type(&self, proto_fqn: &str, current_package: &str) -> Result<TokenStream> {
        use buffa_codegen::context::SENTINEL_MOD;
        let (to_package, within) =
            match self
                .ctx
                .rust_type_relative_split(proto_fqn, current_package, 0)
            {
                Some(s) => {
                    self.check_extern_coverage(proto_fqn, &s.to_package)?;
                    (s.to_package, s.within_package)
                }
                None => (
                    String::new(),
                    self.fallback_unresolved(proto_fqn)?.to_string(),
                ),
            };
        let prefix = if to_package.is_empty() {
            format!("{SENTINEL_MOD}::view")
        } else {
            format!("{to_package}::{SENTINEL_MOD}::view")
        };
        Ok(rust_path_to_tokens(&format!("{prefix}::{within}View")))
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
    encodable_seen: &mut std::collections::BTreeSet<String>,
) -> Result<TokenStream> {
    let mut tokens = TokenStream::new();

    // All types in generated code use fully qualified paths (e.g.
    // `::std::sync::Arc`, `::connectrpc::Context`) so that multiple service
    // files can be `include!`d into the same module without E0252 duplicate
    // import errors.

    tokens.extend(generate_encodable_view_impls(
        file,
        resolver,
        encodable_seen,
    )?);

    for service in &file.service {
        tokens.extend(generate_service(file, service, resolver)?);
    }

    Ok(tokens)
}

/// Emit `impl Encodable<M> for MView<'_>` and
/// `impl Encodable<M> for OwnedView<MView<'static>>` for every distinct
/// RPC output type not already in `seen` (proto FQN).
///
/// These can't be runtime blankets (the `M: Message + Serialize` blanket
/// in `connectrpc::response` would conflict by coherence), so they're
/// emitted per concrete type. Orphan rules allow it because `M` (a local
/// type) appears in the trait parameters.
///
/// `seen` is owned by the caller's batch loop so an output type
/// referenced from multiple input files only gets one impl pair (the
/// stitcher would otherwise hit E0119).
///
/// Skipped for output types that resolve to an absolute (`::`) extern
/// path, since those are foreign and would violate orphan rules.
fn generate_encodable_view_impls(
    file: &FileDescriptorProto,
    resolver: &TypeResolver<'_>,
    seen: &mut std::collections::BTreeSet<String>,
) -> Result<TokenStream> {
    let package = file.package.as_deref().unwrap_or("");
    let mut out = TokenStream::new();
    for service in &file.service {
        for m in &service.method {
            let fqn = m.output_type.as_deref().unwrap_or("");
            if !seen.insert(fqn.to_string()) {
                continue;
            }
            let path = resolver.resolve_path(fqn, package)?;
            // Skip foreign types (extern_path → `::crate_name::...`): the
            // impl would be an orphan in the user's crate.
            if path.starts_with("::") {
                continue;
            }
            let owned = resolver.rust_type(fqn, package)?;
            let view = resolver.rust_view_type(fqn, package)?;
            out.extend(quote! {
                impl ::connectrpc::Encodable<#owned> for #view<'_> {
                    fn encode(&self, codec: ::connectrpc::CodecFormat)
                        -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError>
                    {
                        ::connectrpc::encode_view_body(self, codec)
                    }
                }
                impl ::connectrpc::Encodable<#owned> for ::buffa::view::OwnedView<#view<'static>> {
                    fn encode(&self, codec: ::connectrpc::CodecFormat)
                        -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError>
                    {
                        ::connectrpc::encode_view_body(&**self, codec)
                    }
                }
            });
        }
    }
    Ok(out)
}

/// Generate code for a single service.
/// Reject RPC method sets whose generated Rust identifiers collide.
///
/// Each proto method `Foo` produces both `foo` and `foo_with_options` on the
/// client. Two methods that normalize to the same snake_case name (e.g.
/// `GetFoo` and `get_foo`), or one whose snake form equals another's
/// `_with_options` form, would emit duplicate definitions and fail to
/// compile with an error pointing at generated code rather than the proto.
fn check_method_collisions(service_name: &str, service: &ServiceDescriptorProto) -> Result<()> {
    let mut seen: HashMap<String, String> = HashMap::new();
    for m in &service.method {
        let proto_name = m.name.as_deref().unwrap_or("");
        let snake = proto_name.to_snake_case();
        let with_opts = format!("{snake}_with_options");
        for ident in [snake.as_str(), with_opts.as_str()] {
            if let Some(prev) = seen.get(ident) {
                anyhow::bail!(
                    "service {service_name}: RPC methods {prev:?} and {proto_name:?} \
                     both generate Rust identifier `{ident}`; rename one in the proto"
                );
            }
        }
        seen.insert(snake, proto_name.to_string());
        seen.insert(with_opts, proto_name.to_string());
    }
    Ok(())
}

fn generate_service(
    file: &FileDescriptorProto,
    service: &ServiceDescriptorProto,
    resolver: &TypeResolver<'_>,
) -> Result<TokenStream> {
    let package = file.package.as_deref().unwrap_or("");
    let service_name = service.name.as_deref().unwrap_or("");
    check_method_collisions(service_name, service)?;
    // Empty package is valid proto; the fully-qualified service name is just
    // `ServiceName`, not `.ServiceName` (which would break interop).
    let full_service_name = if package.is_empty() {
        service_name.to_string()
    } else {
        format!("{package}.{service_name}")
    };
    let service_upper = service_name.to_upper_camel_case();
    // `Self` is the only PascalCase Rust keyword, and cannot be a raw ident;
    // suffix it so `service Self {}` (accepted by protoc) generates a valid
    // trait. The suffixed derivatives below are already keyword-safe.
    let trait_name = if service_upper == "Self" {
        format_ident!("Self_")
    } else {
        format_ident!("{}", service_upper)
    };
    let ext_trait_name = format_ident!("{}Ext", service_upper);
    let client_name = format_ident!("{}Client", service_upper);
    let server_name = format_ident!("{}Server", service_upper);
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
            let method_snake = make_field_ident(&method_name.to_snake_case());

            let client_streaming = m.client_streaming.unwrap_or(false);
            let server_streaming = m.server_streaming.unwrap_or(false);

            if server_streaming && !client_streaming {
                // Server streaming method
                quote! {
                    .route_view_server_stream(
                        #service_name_const,
                        #method_name,
                        ::connectrpc::view_streaming_handler_fn({
                            let svc = ::std::sync::Arc::clone(&self);
                            move |ctx, req| {
                                let svc = ::std::sync::Arc::clone(&svc);
                                async move { svc.#method_snake(ctx, req).await }
                            }
                        }),
                    )
                }
            } else if client_streaming && !server_streaming {
                // Client streaming method
                let output_type = resolver
                    .rust_type(m.output_type.as_deref().unwrap_or(""), package)
                    .unwrap();
                quote! {
                    .route_view_client_stream(
                        #service_name_const,
                        #method_name,
                        ::connectrpc::view_client_streaming_handler_fn({
                            let svc = ::std::sync::Arc::clone(&self);
                            move |ctx, req, format| {
                                let svc = ::std::sync::Arc::clone(&svc);
                                async move {
                                    svc.#method_snake(ctx, req).await?.encode::<#output_type>(format)
                                }
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
                        ::connectrpc::view_bidi_streaming_handler_fn({
                            let svc = ::std::sync::Arc::clone(&self);
                            move |ctx, req| {
                                let svc = ::std::sync::Arc::clone(&svc);
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
                let output_type = resolver
                    .rust_type(m.output_type.as_deref().unwrap_or(""), package)
                    .unwrap();

                quote! {
                    .#route_method(
                        #service_name_const,
                        #method_name,
                        {
                            let svc = ::std::sync::Arc::clone(&self);
                            ::connectrpc::view_handler_fn(move |ctx, req, format| {
                                let svc = ::std::sync::Arc::clone(&svc);
                                async move {
                                    svc.#method_snake(ctx, req).await?.encode::<#output_type>(format)
                                }
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
        .map(|m| {
            generate_client_method(
                &service_name_const,
                &full_service_name,
                m,
                resolver,
                package,
            )
        })
        .collect::<Result<Vec<_>>>()?;

    // Generate monomorphic FooServiceServer<T> dispatcher.
    let service_server = generate_service_server(
        &full_service_name,
        &trait_name,
        &server_name,
        service,
        resolver,
        package,
    )?;

    // Example method name for client doc
    let example_method = service
        .method
        .first()
        .and_then(|m| m.name.as_deref())
        .map(|n| make_field_ident(&n.to_snake_case()).to_string())
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
            fn register(self: ::std::sync::Arc<Self>, router: ::connectrpc::Router) -> ::connectrpc::Router;
        }

        impl<S: #trait_name> #ext_trait_name for S {
            fn register(self: ::std::sync::Arc<Self>, router: ::connectrpc::Router) -> ::connectrpc::Router {
                router
                    #(#route_registrations)*
            }
        }

        #service_server

        #client_doc_tokens
        #[derive(Clone)]
        pub struct #client_name<T> {
            transport: T,
            config: ::connectrpc::client::ClientConfig,
        }

        impl<T> #client_name<T>
        where
            T: ::connectrpc::client::ClientTransport,
            <T::ResponseBody as ::http_body::Body>::Error: ::std::fmt::Display,
        {
            /// Create a new client with the given transport and configuration.
            pub fn new(transport: T, config: ::connectrpc::client::ClientConfig) -> Self {
                Self { transport, config }
            }

            /// Get the client configuration.
            pub fn config(&self) -> &::connectrpc::client::ClientConfig {
                &self.config
            }

            /// Get a mutable reference to the client configuration.
            pub fn config_mut(&mut self) -> &mut ::connectrpc::client::ClientConfig {
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
    server_name: &proc_macro2::Ident,
    service: &ServiceDescriptorProto,
    resolver: &TypeResolver<'_>,
    package: &str,
) -> Result<TokenStream> {
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
                quote! { ::connectrpc::dispatcher::codegen::MethodDescriptor::bidi_streaming() }
            } else if client_streaming {
                quote! { ::connectrpc::dispatcher::codegen::MethodDescriptor::client_streaming() }
            } else if server_streaming {
                quote! { ::connectrpc::dispatcher::codegen::MethodDescriptor::server_streaming() }
            } else {
                quote! { ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(#is_idempotent) }
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
        let method_snake = make_field_ident(&method_name.to_snake_case());
        let input_view = resolver.rust_view_type(m.input_type.as_deref().unwrap_or(""), package)?;
        let output_type = resolver.rust_type(m.output_type.as_deref().unwrap_or(""), package)?;
        let cs = m.client_streaming.unwrap_or(false);
        let ss = m.server_streaming.unwrap_or(false);

        if cs && ss {
            // Bidi streaming
            call_bidi_arms.push(quote! {
                #method_name => {
                    let svc = ::std::sync::Arc::clone(&self.inner);
                    Box::pin(async move {
                        let req_stream = ::connectrpc::dispatcher::codegen::decode_view_request_stream::<#input_view>(requests, format);
                        let resp = svc.#method_snake(ctx, req_stream).await?;
                        Ok(resp.map_body(|s| ::connectrpc::dispatcher::codegen::encode_response_stream(s, format)))
                    })
                }
            });
        } else if cs {
            // Client streaming
            call_cs_arms.push(quote! {
                #method_name => {
                    let svc = ::std::sync::Arc::clone(&self.inner);
                    Box::pin(async move {
                        let req_stream = ::connectrpc::dispatcher::codegen::decode_view_request_stream::<#input_view>(requests, format);
                        svc.#method_snake(ctx, req_stream).await?.encode::<#output_type>(format)
                    })
                }
            });
        } else if ss {
            // Server streaming
            call_ss_arms.push(quote! {
                #method_name => {
                    let svc = ::std::sync::Arc::clone(&self.inner);
                    Box::pin(async move {
                        let req = ::connectrpc::dispatcher::codegen::decode_request_view::<#input_view>(request, format)?;
                        let resp = svc.#method_snake(ctx, req).await?;
                        Ok(resp.map_body(|s| ::connectrpc::dispatcher::codegen::encode_response_stream(s, format)))
                    })
                }
            });
        } else {
            // Unary
            call_unary_arms.push(quote! {
                #method_name => {
                    let svc = ::std::sync::Arc::clone(&self.inner);
                    Box::pin(async move {
                        let req = ::connectrpc::dispatcher::codegen::decode_request_view::<#input_view>(request, format)?;
                        svc.#method_snake(ctx, req).await?.encode::<#output_type>(format)
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
            inner: ::std::sync::Arc<T>,
        }

        impl<T: #trait_name> #server_name<T> {
            /// Wrap a service implementation in a monomorphic dispatcher.
            pub fn new(service: T) -> Self {
                Self { inner: ::std::sync::Arc::new(service) }
            }

            /// Wrap an already-`Arc`'d service implementation.
            pub fn from_arc(inner: ::std::sync::Arc<T>) -> Self {
                Self { inner }
            }
        }

        impl<T> Clone for #server_name<T> {
            fn clone(&self) -> Self {
                Self { inner: ::std::sync::Arc::clone(&self.inner) }
            }
        }

        impl<T: #trait_name> ::connectrpc::Dispatcher for #server_name<T> {
            #[inline]
            fn lookup(&self, path: &str) -> Option<::connectrpc::dispatcher::codegen::MethodDescriptor> {
                let method = path.strip_prefix(#path_prefix)?;
                match method {
                    #(#lookup_arms)*
                    _ => None,
                }
            }

            fn call_unary(
                &self,
                path: &str,
                ctx: ::connectrpc::RequestContext,
                request: ::buffa::bytes::Bytes,
                format: ::connectrpc::CodecFormat,
            ) -> ::connectrpc::dispatcher::codegen::UnaryResult {
                let Some(method) = path.strip_prefix(#path_prefix) else {
                    return ::connectrpc::dispatcher::codegen::unimplemented_unary(path);
                };
                // Suppress unused warnings when this service has no unary methods.
                let _ = (&ctx, &request, &format);
                match method {
                    #(#call_unary_arms)*
                    _ => ::connectrpc::dispatcher::codegen::unimplemented_unary(path),
                }
            }

            fn call_server_streaming(
                &self,
                path: &str,
                ctx: ::connectrpc::RequestContext,
                request: ::buffa::bytes::Bytes,
                format: ::connectrpc::CodecFormat,
            ) -> ::connectrpc::dispatcher::codegen::StreamingResult {
                let Some(method) = path.strip_prefix(#path_prefix) else {
                    return ::connectrpc::dispatcher::codegen::unimplemented_streaming(path);
                };
                let _ = (&ctx, &request, &format);
                match method {
                    #(#call_ss_arms)*
                    _ => ::connectrpc::dispatcher::codegen::unimplemented_streaming(path),
                }
            }

            fn call_client_streaming(
                &self,
                path: &str,
                ctx: ::connectrpc::RequestContext,
                requests: ::connectrpc::dispatcher::codegen::RequestStream,
                format: ::connectrpc::CodecFormat,
            ) -> ::connectrpc::dispatcher::codegen::UnaryResult {
                let Some(method) = path.strip_prefix(#path_prefix) else {
                    return ::connectrpc::dispatcher::codegen::unimplemented_unary(path);
                };
                let _ = (&ctx, &requests, &format);
                match method {
                    #(#call_cs_arms)*
                    _ => ::connectrpc::dispatcher::codegen::unimplemented_unary(path),
                }
            }

            fn call_bidi_streaming(
                &self,
                path: &str,
                ctx: ::connectrpc::RequestContext,
                requests: ::connectrpc::dispatcher::codegen::RequestStream,
                format: ::connectrpc::CodecFormat,
            ) -> ::connectrpc::dispatcher::codegen::StreamingResult {
                let Some(method) = path.strip_prefix(#path_prefix) else {
                    return ::connectrpc::dispatcher::codegen::unimplemented_streaming(path);
                };
                let _ = (&ctx, &requests, &format);
                match method {
                    #(#call_bidi_arms)*
                    _ => ::connectrpc::dispatcher::codegen::unimplemented_streaming(path),
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
    let method_snake = make_field_ident(&method_name.to_snake_case());
    let input_view_type =
        resolver.rust_view_type(method.input_type.as_deref().unwrap_or(""), package)?;
    let output_type = resolver.rust_type(method.output_type.as_deref().unwrap_or(""), package)?;

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
                ctx: ::connectrpc::RequestContext,
                request: ::buffa::view::OwnedView<#input_view_type<'static>>,
            ) -> impl ::std::future::Future<Output = ::connectrpc::ServiceResult<::connectrpc::ServiceStream<#output_type>>> + Send;
        })
    } else if client_streaming && !server_streaming {
        // Client streaming method
        Ok(quote! {
            #method_doc_tokens
            fn #method_snake<'a>(
                &'a self,
                ctx: ::connectrpc::RequestContext,
                requests: ::connectrpc::ServiceStream<::buffa::view::OwnedView<#input_view_type<'static>>>,
            ) -> impl ::std::future::Future<Output = ::connectrpc::ServiceResult<impl ::connectrpc::Encodable<#output_type> + Send + use<'a, Self>>> + Send;
        })
    } else if client_streaming && server_streaming {
        // Bidi streaming method
        Ok(quote! {
            #method_doc_tokens
            fn #method_snake(
                &self,
                ctx: ::connectrpc::RequestContext,
                requests: ::connectrpc::ServiceStream<::buffa::view::OwnedView<#input_view_type<'static>>>,
            ) -> impl ::std::future::Future<Output = ::connectrpc::ServiceResult<::connectrpc::ServiceStream<#output_type>>> + Send;
        })
    } else {
        // Unary method
        Ok(quote! {
            #method_doc_tokens
            fn #method_snake<'a>(
                &'a self,
                ctx: ::connectrpc::RequestContext,
                request: ::buffa::view::OwnedView<#input_view_type<'static>>,
            ) -> impl ::std::future::Future<Output = ::connectrpc::ServiceResult<impl ::connectrpc::Encodable<#output_type> + Send + use<'a, Self>>> + Send;
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
    service_name_const: &Ident,
    full_service_name: &str,
    method: &MethodDescriptorProto,
    resolver: &TypeResolver<'_>,
    package: &str,
) -> Result<TokenStream> {
    let method_name = method.name.as_deref().unwrap_or("");
    let method_snake = make_field_ident(&method_name.to_snake_case());
    let method_with_opts = format_ident!("{}_with_options", method_name.to_snake_case());
    let input_type = resolver.rust_type(method.input_type.as_deref().unwrap_or(""), package)?;
    let output_view_type =
        resolver.rust_view_type(method.output_type.as_deref().unwrap_or(""), package)?;

    let client_streaming = method.client_streaming.unwrap_or(false);
    let server_streaming = method.server_streaming.unwrap_or(false);

    let doc = format!(
        " Call the {method_name} RPC. Sends a request to /{full_service_name}/{method_name}."
    );
    let doc_opts = format!(
        " Call the {method_name} RPC with explicit per-call options. \
         Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults."
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
                ::connectrpc::client::UnaryResponse<::buffa::view::OwnedView<#output_view_type<'static>>>,
                ::connectrpc::ConnectError,
            >
        };
        call_body = quote! {
            ::connectrpc::client::call_client_stream(
                &self.transport, &self.config,
                #service_name_const, #method_name,
                requests, options,
            ).await
        };
        short_args = quote! { requests: impl IntoIterator<Item = #input_type> };
        opts_args = quote! { requests: impl IntoIterator<Item = #input_type>, options: ::connectrpc::client::CallOptions };
        short_delegate_args = quote! { requests, ::connectrpc::client::CallOptions::default() };
    } else if client_streaming && server_streaming {
        // Bidi
        ret_ty = quote! {
            Result<
                ::connectrpc::client::BidiStream<
                    T::ResponseBody, #input_type, #output_view_type<'static>
                >,
                ::connectrpc::ConnectError,
            >
        };
        call_body = quote! {
            ::connectrpc::client::call_bidi_stream(
                &self.transport, &self.config,
                #service_name_const, #method_name, options,
            ).await
        };
        short_args = quote! {};
        opts_args = quote! { options: ::connectrpc::client::CallOptions };
        short_delegate_args = quote! { ::connectrpc::client::CallOptions::default() };
    } else if server_streaming {
        // Server-stream
        ret_ty = quote! {
            Result<
                ::connectrpc::client::ServerStream<T::ResponseBody, #output_view_type<'static>>,
                ::connectrpc::ConnectError,
            >
        };
        call_body = quote! {
            ::connectrpc::client::call_server_stream(
                &self.transport, &self.config,
                #service_name_const, #method_name,
                request, options,
            ).await
        };
        short_args = quote! { request: #input_type };
        opts_args = quote! { request: #input_type, options: ::connectrpc::client::CallOptions };
        short_delegate_args = quote! { request, ::connectrpc::client::CallOptions::default() };
    } else {
        // Unary
        ret_ty = quote! {
            Result<
                ::connectrpc::client::UnaryResponse<::buffa::view::OwnedView<#output_view_type<'static>>>,
                ::connectrpc::ConnectError,
            >
        };
        call_body = quote! {
            ::connectrpc::client::call_unary(
                &self.transport, &self.config,
                #service_name_const, #method_name,
                request, options,
            ).await
        };
        short_args = quote! { request: #input_type };
        opts_args = quote! { request: #input_type, options: ::connectrpc::client::CallOptions };
        short_delegate_args = quote! { request, ::connectrpc::client::CallOptions::default() };
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
        minimal_file_with_method(package, "Ping", input_type, output_type, local_messages)
    }

    /// Like [`minimal_file`] but with a custom RPC method name, for testing
    /// keyword collisions and other name-derived behaviour.
    fn minimal_file_with_method(
        package: Option<&str>,
        method_name: &str,
        input_type: &str,
        output_type: &str,
        local_messages: &[&str],
    ) -> FileDescriptorProto {
        let method = MethodDescriptorProto {
            name: Some(method_name.into()),
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

    /// Build a minimal proto file with one service holding the given method
    /// names, all typed `Empty` -> `Empty`. Used for collision tests where
    /// the method *names* are what's under test.
    fn minimal_file_with_methods(package: &str, method_names: &[&str]) -> FileDescriptorProto {
        let methods = method_names
            .iter()
            .map(|n| MethodDescriptorProto {
                name: Some((*n).into()),
                input_type: Some(format!(".{package}.Empty")),
                output_type: Some(format!(".{package}.Empty")),
                ..Default::default()
            })
            .collect();
        let service = ServiceDescriptorProto {
            name: Some("PingService".into()),
            method: methods,
            ..Default::default()
        };
        FileDescriptorProto {
            name: Some("ping.proto".into()),
            package: Some(package.into()),
            service: vec![service],
            message_type: vec![DescriptorProto {
                name: Some("Empty".into()),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// Generate service code for `files[target_idx]`. All files are visible
    /// to the resolver (as transitive deps via `--include_imports`), but
    /// only the target is in `file_to_generate` — mirroring real protoc use.
    ///
    /// `extern_paths` is wired into `CodeGenConfig.extern_paths` (which
    /// feeds the resolver's type_map via `effective_extern_paths`).
    /// `require_extern` selects unified (`false`, super::-relative) vs
    /// split (`true`, absolute-only) mode.
    fn gen_service(
        files: &[FileDescriptorProto],
        target_idx: usize,
        extern_paths: &[(String, String)],
        require_extern: bool,
    ) -> Result<String> {
        let mut config = buffa_codegen::CodeGenConfig::default();
        config.extern_paths = extern_paths.to_vec();
        let target_name = files[target_idx]
            .name
            .clone()
            .into_iter()
            .collect::<Vec<_>>();
        let resolver = TypeResolver::new(files, &target_name, &config, require_extern);
        let file = &files[target_idx];
        let service = &file.service[0];
        Ok(generate_service(file, service, &resolver)?.to_string())
    }

    /// Assert that `formatted` (a Rust source string) contains no `use`
    /// items at the file root. Parses with `syn` rather than string-matching
    /// so doc comments, string literals, and indented `use` statements in
    /// nested modules cannot trigger false positives.
    fn assert_no_top_level_use(formatted: &str, label: &str) {
        let parsed: syn::File = syn::parse_str(formatted).expect("formatted code parses");
        let offenders: Vec<String> = parsed
            .items
            .iter()
            .filter_map(|item| match item {
                syn::Item::Use(u) => Some(quote!(#u).to_string()),
                _ => None,
            })
            .collect();
        assert!(
            offenders.is_empty(),
            "{label} contains top-level use statement(s): {offenders:?}\nFull source:\n{formatted}"
        );
    }

    fn gen_file(
        files: &[FileDescriptorProto],
        target_idx: usize,
        extern_paths: &[(String, String)],
        require_extern: bool,
    ) -> Result<String> {
        let mut config = buffa_codegen::CodeGenConfig::default();
        config.extern_paths = extern_paths.to_vec();
        let target_name = files[target_idx]
            .name
            .clone()
            .into_iter()
            .collect::<Vec<_>>();
        let resolver = TypeResolver::new(files, &target_name, &config, require_extern);
        let mut seen = std::collections::BTreeSet::new();
        Ok(generate_connect_services(&files[target_idx], &resolver, &mut seen)?.to_string())
    }

    #[test]
    fn unary_response_body_captures_self_lifetime() {
        let file = minimal_file(
            Some("example.v1"),
            ".example.v1.PingReq",
            ".example.v1.PingResp",
            &["PingReq", "PingResp"],
        );
        let code = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap();
        assert!(code.contains("< 'a >"), "trait method missing 'a: {code}");
        assert!(code.contains("& 'a self"), "missing &'a self: {code}");
        assert!(
            code.contains("use < 'a , Self >"),
            "missing use<'a, Self> capture: {code}"
        );
        assert!(
            !code.contains("'static + use"),
            "'static bound on body should be dropped: {code}"
        );
    }

    #[test]
    fn encodable_view_impls_emitted_per_output_type() {
        let file = minimal_file(
            Some("example.v1"),
            ".example.v1.PingReq",
            ".example.v1.PingResp",
            &["PingReq", "PingResp"],
        );
        let code = gen_file(std::slice::from_ref(&file), 0, &[], false).unwrap();
        assert!(
            code.contains(
                ":: connectrpc :: Encodable < PingResp > for __buffa :: view :: PingRespView"
            ),
            "missing Encodable<PingResp> for PingRespView: {code}"
        );
        assert!(
            code.contains(
                ":: connectrpc :: Encodable < PingResp > for :: buffa :: view :: OwnedView"
            ),
            "missing Encodable<PingResp> for OwnedView<PingRespView>: {code}"
        );
        // Input type should NOT get an impl (only output types).
        assert!(!code.contains("Encodable < PingReq >"), "got: {code}");
    }

    #[test]
    fn encodable_view_impls_skipped_for_extern_output() {
        // Output type resolves via the WKT extern_path → ::buffa_types::...
        // so the impl would be an orphan; verify it's skipped.
        let wkt = FileDescriptorProto {
            name: Some("google/protobuf/empty.proto".into()),
            package: Some("google.protobuf".into()),
            message_type: vec![DescriptorProto {
                name: Some("Empty".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let file = minimal_file(
            Some("example.v1"),
            ".example.v1.PingReq",
            ".google.protobuf.Empty",
            &["PingReq"],
        );
        let code = gen_file(&[wkt, file], 1, &[], false).unwrap();
        // The impl bodies call encode_view_body; the trait method's
        // `impl Encodable<M>` RPITIT bound doesn't.
        assert!(
            !code.contains("encode_view_body"),
            "extern output type must not get Encodable impl: {code}"
        );
    }

    #[test]
    fn encodable_view_impls_deduped_across_files() {
        // Two service files in different packages both return
        // `.common.v1.Reply`. The stitcher mounts both files into one
        // module tree, so the Encodable<Reply> impls must be emitted
        // exactly once across the batch (else E0119).
        let common = FileDescriptorProto {
            name: Some("common.proto".into()),
            package: Some("common.v1".into()),
            message_type: vec![DescriptorProto {
                name: Some("Reply".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let svc = |name: &str, pkg: &str| FileDescriptorProto {
            name: Some(name.into()),
            package: Some(pkg.into()),
            message_type: vec![DescriptorProto {
                name: Some("Req".into()),
                ..Default::default()
            }],
            service: vec![ServiceDescriptorProto {
                name: Some("S".into()),
                method: vec![MethodDescriptorProto {
                    name: Some("Call".into()),
                    input_type: Some(format!(".{pkg}.Req")),
                    output_type: Some(".common.v1.Reply".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let files = vec![common, svc("a.proto", "a.v1"), svc("b.proto", "b.v1")];

        let generated = generate_files(
            &files,
            &["a.proto".into(), "b.proto".into()],
            &Options::default(),
        )
        .unwrap();
        let combined: String = generated
            .iter()
            .filter(|f| f.kind == GeneratedFileKind::Owned)
            .map(|f| f.content.as_str())
            .collect();

        let view_impl = "impl ::connectrpc::Encodable<super::super::common::v1::Reply>\nfor super::super::common::v1::__buffa::view::ReplyView<'_>";
        let owned_view_impl = "impl ::connectrpc::Encodable<super::super::common::v1::Reply>\nfor ::buffa::view::OwnedView<";
        assert_eq!(
            combined.matches(view_impl).count(),
            1,
            "Encodable<Reply> for ReplyView<'_> must appear once: {combined}"
        );
        assert_eq!(
            combined.matches(owned_view_impl).count(),
            1,
            "Encodable<Reply> for OwnedView<ReplyView> must appear once: {combined}"
        );
    }

    #[test]
    fn service_name_with_package() {
        let file = minimal_file(
            Some("example.v1"),
            ".example.v1.PingReq",
            ".example.v1.PingResp",
            &["PingReq", "PingResp"],
        );
        let code = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap();
        assert!(code.contains("\"example.v1.PingService\""), "got: {code}");
    }

    #[test]
    fn service_name_without_package() {
        // Empty package must produce "PingService", not ".PingService".
        let file = minimal_file(None, ".PingReq", ".PingResp", &["PingReq", "PingResp"]);
        let code = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap();
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
        let code = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap();
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
        let code = gen_service(&[common, svc], 1, &[], false).unwrap();

        // example.v1 -> super::super -> common::v1::Shared
        // (token stream stringifies `::` with spaces, so match loosely)
        assert!(
            code.contains("super :: super :: common :: v1 :: Shared"),
            "cross-package path not emitted: {code}"
        );
        assert!(
            code.contains("super :: super :: common :: v1 :: __buffa :: view :: SharedView"),
            "cross-package view path not emitted: {code}"
        );
    }

    #[test]
    fn nested_message_view_type_mirrors_owned_module_nesting() {
        // Service in example.v1 references Outer.Inner (nested under Outer).
        // buffa lays out the view as __buffa::view::outer::InnerView, mirroring
        // the owned outer::Inner layout. rust_view_type must insert the
        // sentinel at the package boundary, not at the type boundary.
        let file = FileDescriptorProto {
            name: Some("nested.proto".into()),
            package: Some("example.v1".into()),
            message_type: vec![
                DescriptorProto {
                    name: Some("Outer".into()),
                    nested_type: vec![DescriptorProto {
                        name: Some("Inner".into()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("Out".into()),
                    ..Default::default()
                },
            ],
            service: vec![ServiceDescriptorProto {
                name: Some("NestedService".into()),
                method: vec![MethodDescriptorProto {
                    name: Some("Ping".into()),
                    input_type: Some(".example.v1.Outer.Inner".into()),
                    output_type: Some(".example.v1.Out".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let code = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap();

        assert!(
            code.contains("__buffa :: view :: outer :: InnerView"),
            "nested view path not emitted: {code}"
        );
        assert!(
            code.contains("outer :: Inner"),
            "nested owned path not emitted: {code}"
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
        let code = gen_service(&[wkt, svc], 1, &[], false).unwrap();

        assert!(
            code.contains(":: buffa_types :: google :: protobuf :: Empty"),
            "WKT extern path not emitted: {code}"
        );
    }

    #[test]
    fn extern_catchall_uses_absolute_paths() {
        let file = minimal_file(
            Some("example.v1"),
            ".example.v1.PingReq",
            ".example.v1.PingResp",
            &["PingReq", "PingResp"],
        );
        let extern_paths = [(".".into(), "crate::proto".into())];
        let code = gen_service(std::slice::from_ref(&file), 0, &extern_paths, true).unwrap();
        assert!(
            code.contains("crate :: proto :: example :: v1 :: PingReq"),
            "owned type path missing: {code}"
        );
        assert!(
            code.contains("crate :: proto :: example :: v1 :: __buffa :: view :: PingReqView"),
            "view type path missing: {code}"
        );
    }

    #[test]
    fn extern_catchall_with_wkt_longest_wins() {
        // Auto-injected `.google.protobuf` mapping is more specific than
        // the `.` catch-all, so WKTs still route to ::buffa_types.
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
        let extern_paths = [(".".into(), "crate::proto".into())];
        let code = gen_service(&[wkt, svc], 1, &extern_paths, true).unwrap();
        assert!(
            code.contains(":: buffa_types :: google :: protobuf :: Empty"),
            "WKT mapping lost to catch-all: {code}"
        );
        assert!(
            code.contains("crate :: proto :: example :: v1 :: Out"),
            "local type not routed through catch-all: {code}"
        );
    }

    #[test]
    fn missing_extern_path_errors() {
        let file = minimal_file(
            Some("example.v1"),
            ".example.v1.PingReq",
            ".example.v1.PingResp",
            &["PingReq", "PingResp"],
        );
        let err = gen_service(std::slice::from_ref(&file), 0, &[], true).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("extern_path"),
            "error message lacks hint: {msg}"
        );
    }

    #[test]
    fn keyword_package_escaped() {
        // `google.type` -> `google::r#type` via idents::rust_path_to_tokens.
        let file = minimal_file(
            Some("google.type"),
            ".google.type.LatLng",
            ".google.type.LatLng",
            &["LatLng"],
        );
        let extern_paths = [(".".into(), "crate::proto".into())];
        let code = gen_service(std::slice::from_ref(&file), 0, &extern_paths, true).unwrap();
        assert!(
            code.contains("crate :: proto :: google :: r#type :: LatLng"),
            "keyword segment not escaped: {code}"
        );
    }

    #[test]
    fn keyword_method_escaped() {
        // `rpc Move(...)` -> snake_case `move` is a Rust keyword; emit `r#move`
        // via idents::make_field_ident. Regression for issue #23.
        let file = minimal_file_with_method(
            Some("example.v1"),
            "Move",
            ".example.v1.Empty",
            ".example.v1.Empty",
            &["Empty"],
        );
        let code = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap();
        assert!(
            code.contains("fn r#move"),
            "keyword method not escaped: {code}"
        );
        assert!(
            code.contains("move_with_options"),
            "suffixed variant should not need escaping: {code}"
        );
        // Doc example should also use the escaped form so the snippet is valid.
        assert!(code.contains("client.r#move(request)"));
        syn::parse_str::<syn::File>(&code).expect("generated code parses");
    }

    #[test]
    fn path_keyword_method_suffixed() {
        // `self`/`super`/`Self`/`crate` cannot be raw identifiers; they are
        // suffixed with `_` instead (matching prost convention).
        let file = minimal_file_with_method(
            Some("example.v1"),
            "Self",
            ".example.v1.Empty",
            ".example.v1.Empty",
            &["Empty"],
        );
        let code = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap();
        assert!(
            code.contains("fn self_"),
            "path-keyword method not suffixed: {code}"
        );
        // The `_with_options` variant uses the unsuffixed snake name; the
        // suffix already de-keywords it, so we get `self_with_options`
        // (not `self__with_options`).
        assert!(code.contains("self_with_options"));
        syn::parse_str::<syn::File>(&code).expect("generated code parses");
    }

    #[test]
    fn service_name_keyword_suffixed() {
        // `service Self {}` is accepted by protoc but `Self` is a Rust keyword
        // that cannot be a raw ident; the bare trait name is suffixed `Self_`
        // while the derived `SelfExt`/`SelfClient`/`SelfServer` are already safe.
        let mut file = minimal_file(
            Some("example.v1"),
            ".example.v1.Empty",
            ".example.v1.Empty",
            &["Empty"],
        );
        file.service[0].name = Some("Self".into());
        let code = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap();
        assert!(code.contains("trait Self_ "), "trait not suffixed: {code}");
        assert!(code.contains("trait SelfExt"));
        assert!(code.contains("struct SelfClient"));
        assert!(code.contains("struct SelfServer"));
        syn::parse_str::<syn::File>(&code).expect("generated code parses");
    }

    #[test]
    fn method_snake_collision_errors() {
        // protoc accepts `GetFoo` and `get_foo` in the same service; both
        // snake-case to `get_foo`, which would emit duplicate Rust methods.
        let file = minimal_file_with_methods("example.v1", &["GetFoo", "get_foo"]);
        let err = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("PingService"), "missing service name: {msg}");
        assert!(msg.contains("\"GetFoo\""), "missing first method: {msg}");
        assert!(msg.contains("\"get_foo\""), "missing second method: {msg}");
        assert!(msg.contains("`get_foo`"), "missing rust ident: {msg}");
    }

    #[test]
    fn method_with_options_collision_errors() {
        // `Ping` generates client method `ping_with_options`; a proto method
        // `PingWithOptions` would generate the same base name.
        let file = minimal_file_with_methods("example.v1", &["Ping", "PingWithOptions"]);
        let err = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("\"Ping\""), "missing first method: {msg}");
        assert!(
            msg.contains("\"PingWithOptions\""),
            "missing second method: {msg}"
        );
        assert!(
            msg.contains("`ping_with_options`"),
            "missing rust ident: {msg}"
        );
    }

    #[test]
    fn distinct_methods_do_not_collide() {
        let file = minimal_file_with_methods("example.v1", &["GetFoo", "GetBar"]);
        let code = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap();
        syn::parse_str::<syn::File>(&code).expect("generated code parses");
    }

    #[test]
    fn options_default_buffa_config() {
        let cfg = Options::default().to_buffa_config();
        assert!(cfg.generate_json, "connectrpc enables JSON by default");
        assert!(cfg.generate_views);
        assert!(cfg.emit_register_fn);
        assert!(!cfg.strict_utf8_mapping);
    }

    #[test]
    fn options_buffa_passthrough_forces_views() {
        let mut opts = Options::default();
        opts.buffa.emit_register_fn = false;
        opts.buffa.generate_views = false;
        let cfg = opts.to_buffa_config();
        assert!(!cfg.emit_register_fn);
        assert!(cfg.generate_views, "generate_views must be forced on");
    }

    #[test]
    fn generate_files_emit_register_fn_false_suppresses_register_types() {
        // Build a file with a single message so buffa would normally emit
        // `pub fn register_types(&mut TypeRegistry)` aggregating it.
        let file = FileDescriptorProto {
            name: Some("ping.proto".into()),
            package: Some("example.v1".into()),
            message_type: vec![DescriptorProto {
                name: Some("PingReq".into()),
                ..Default::default()
            }],
            ..Default::default()
        };

        // `register_types` is emitted into the per-package stitcher, so
        // locate the PackageMod output and check that one.
        let stitcher = |files: &[GeneratedFile]| {
            files
                .iter()
                .find(|f| f.kind == GeneratedFileKind::PackageMod)
                .expect("PackageMod file emitted")
                .content
                .clone()
        };

        let with_fn = generate_files(
            std::slice::from_ref(&file),
            &["ping.proto".into()],
            &Options::default(),
        )
        .unwrap();
        let mod_rs = stitcher(&with_fn);
        assert!(
            mod_rs.contains("fn register_types"),
            "expected register_types in default output: {mod_rs}"
        );

        let mut opts = Options::default();
        opts.buffa.emit_register_fn = false;
        let without_fn =
            generate_files(std::slice::from_ref(&file), &["ping.proto".into()], &opts).unwrap();
        let mod_rs = stitcher(&without_fn);
        assert!(
            !mod_rs.contains("fn register_types"),
            "register_types should be suppressed: {mod_rs}"
        );
    }

    #[test]
    fn plugin_no_register_fn_parses() {
        let request = CodeGeneratorRequest {
            parameter: Some("buffa_module=crate::proto,no_register_fn".into()),
            file_to_generate: vec![],
            proto_file: vec![],
            ..Default::default()
        };
        // Plugin path emits services only, so we can't observe the buffa
        // config directly — just make sure the option parses without error.
        generate(&request).expect("no_register_fn should be a recognized plugin option");
    }

    #[test]
    fn no_top_level_use_statements_in_generated_code() {
        // When multiple service files are `include!`d into the same module,
        // top-level `use` statements cause E0252 (duplicate imports). Verify
        // the generated code uses fully qualified paths instead.
        let file = minimal_file(
            Some("example.v1"),
            ".example.v1.PingReq",
            ".example.v1.PingResp",
            &["PingReq", "PingResp"],
        );
        let code = gen_service(std::slice::from_ref(&file), 0, &[], false).unwrap();
        let formatted = format_token_stream(&code.parse::<TokenStream>().unwrap()).unwrap();
        assert_no_top_level_use(&formatted, "generated code");
    }

    #[test]
    fn multi_service_include_no_e0252() {
        // Simulate `buffa-packaging` including two service files into one
        // module. Both files must parse together without duplicate imports.
        let file_a = {
            let method = MethodDescriptorProto {
                name: Some("Ping".into()),
                input_type: Some(".svc.v1.PingReq".into()),
                output_type: Some(".svc.v1.PingResp".into()),
                ..Default::default()
            };
            let service = ServiceDescriptorProto {
                name: Some("Alpha".into()),
                method: vec![method],
                ..Default::default()
            };
            FileDescriptorProto {
                name: Some("alpha.proto".into()),
                package: Some("svc.v1".into()),
                service: vec![service],
                message_type: vec![
                    DescriptorProto {
                        name: Some("PingReq".into()),
                        ..Default::default()
                    },
                    DescriptorProto {
                        name: Some("PingResp".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }
        };
        let file_b = {
            let method = MethodDescriptorProto {
                name: Some("Pong".into()),
                input_type: Some(".svc.v1.PongReq".into()),
                output_type: Some(".svc.v1.PongResp".into()),
                ..Default::default()
            };
            let service = ServiceDescriptorProto {
                name: Some("Beta".into()),
                method: vec![method],
                ..Default::default()
            };
            FileDescriptorProto {
                name: Some("beta.proto".into()),
                package: Some("svc.v1".into()),
                service: vec![service],
                message_type: vec![
                    DescriptorProto {
                        name: Some("PongReq".into()),
                        ..Default::default()
                    },
                    DescriptorProto {
                        name: Some("PongResp".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }
        };

        let files = vec![file_a, file_b];
        let config = buffa_codegen::CodeGenConfig::default();
        let targets = vec!["alpha.proto".to_string(), "beta.proto".to_string()];
        let resolver = TypeResolver::new(&files, &targets, &config, false);

        let mut seen = std::collections::BTreeSet::new();
        let code_a = generate_connect_services(&files[0], &resolver, &mut seen).unwrap();
        let code_b = generate_connect_services(&files[1], &resolver, &mut seen).unwrap();

        let formatted_a = format_token_stream(&code_a).unwrap();
        let formatted_b = format_token_stream(&code_b).unwrap();

        // Each file independently must parse.
        syn::parse_str::<syn::File>(&formatted_a).expect("service A should parse independently");
        syn::parse_str::<syn::File>(&formatted_b).expect("service B should parse independently");

        // Both files combined into one module must also parse (the E0252 scenario).
        let combined = format!("{formatted_a}\n{formatted_b}");
        syn::parse_str::<syn::File>(&combined)
            .expect("combined services should parse without E0252");

        // No top-level `use` in either file.
        assert_no_top_level_use(&formatted_a, "service A");
        assert_no_top_level_use(&formatted_b, "service B");
    }
}
