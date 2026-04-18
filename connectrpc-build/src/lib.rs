//! Build-time integration for connectrpc.
//!
//! Use this crate in `build.rs` to compile `.proto` files into Rust code at
//! build time. It shells out to `protoc` (or `buf`, or reads a precompiled
//! `FileDescriptorSet`) to obtain descriptors, then runs
//! [`connectrpc_codegen`] to emit buffa message types plus ConnectRPC
//! service traits and clients into `$OUT_DIR`.
//!
//! # Example
//!
//! ```rust,ignore
//! // build.rs
//! fn main() {
//!     connectrpc_build::Config::new()
//!         .files(&["proto/my_service.proto"])
//!         .includes(&["proto/"])
//!         .include_file("_connectrpc.rs")
//!         .compile()
//!         .unwrap();
//! }
//! ```
//!
//! ```rust,ignore
//! // lib.rs
//! include!(concat!(env!("OUT_DIR"), "/_connectrpc.rs"));
//! ```
//!
//! # Requirements
//!
//! Requires `protoc` on `PATH` (or set via `PROTOC`). To use `buf` instead,
//! call [`Config::use_buf`]. To avoid both, precompile a `FileDescriptorSet`
//! once and ship it alongside your source via [`Config::descriptor_set`].

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use buffa::Message;
use buffa_codegen::generated::descriptor::FileDescriptorSet;
use connectrpc_codegen::codegen::{self, Options};

/// How to acquire a `FileDescriptorSet` from `.proto` files.
#[derive(Debug, Clone, Default)]
enum DescriptorSource {
    /// Invoke `protoc` (default). Requires `protoc` on PATH or `PROTOC` env var.
    #[default]
    Protoc,
    /// Invoke `buf build --as-file-descriptor-set`. Requires `buf` on PATH.
    Buf,
    /// Read a pre-built `FileDescriptorSet` from a file.
    Precompiled(PathBuf),
}

/// Builder for configuring and running connectrpc code generation.
///
/// See the [crate-level docs](crate) for a worked example.
pub struct Config {
    files: Vec<PathBuf>,
    includes: Vec<PathBuf>,
    out_dir: Option<PathBuf>,
    descriptor_source: DescriptorSource,
    include_file: Option<String>,
    options: Options,
}

impl Config {
    /// Create a new configuration with defaults.
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            includes: Vec::new(),
            out_dir: None,
            descriptor_source: DescriptorSource::default(),
            include_file: None,
            options: Options::default(),
        }
    }

    /// Add `.proto` files to compile.
    #[must_use]
    pub fn files(mut self, files: &[impl AsRef<Path>]) -> Self {
        self.files
            .extend(files.iter().map(|f| f.as_ref().to_path_buf()));
        self
    }

    /// Add include directories for protoc to search for imports.
    ///
    /// Ignored when using [`Config::use_buf`] (buf resolves imports via
    /// `buf.yaml`).
    #[must_use]
    pub fn includes(mut self, includes: &[impl AsRef<Path>]) -> Self {
        self.includes
            .extend(includes.iter().map(|i| i.as_ref().to_path_buf()));
        self
    }

    /// Set the output directory. Defaults to `$OUT_DIR`.
    #[must_use]
    pub fn out_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.out_dir = Some(dir.into());
        self
    }

    /// Honor `features.utf8_validation = NONE` by emitting `Vec<u8>`/`&[u8]`
    /// for such string fields. See [`Options::strict_utf8_mapping`].
    #[must_use]
    pub fn strict_utf8_mapping(mut self, enabled: bool) -> Self {
        self.options.strict_utf8_mapping = enabled;
        self
    }

    /// Emit `serde` derives and proto3 JSON helpers (default: true).
    ///
    /// Disable only for binary-only clients; the Connect protocol's JSON
    /// codec requires this. See [`Options::generate_json`].
    #[must_use]
    pub fn generate_json(mut self, enabled: bool) -> Self {
        self.options.generate_json = enabled;
        self
    }

    /// Emit the per-file `register_types(&mut TypeRegistry)` aggregator
    /// (default: true).
    ///
    /// Set to `false` when the generated files are `include!`d into the
    /// same module — the identically-named functions would otherwise
    /// collide. See [`Options::emit_register_fn`].
    #[must_use]
    pub fn emit_register_fn(mut self, enabled: bool) -> Self {
        self.options.emit_register_fn = enabled;
        self
    }

    /// Emit `impl ViewEncode<'a>` on generated `*View<'a>` types (default:
    /// false). See [`Options::view_encode`].
    #[must_use]
    pub fn view_encode(mut self, enabled: bool) -> Self {
        self.options.view_encode = enabled;
        self
    }

    /// Field paths whose proto `bytes` type should generate `bytes::Bytes`
    /// instead of `Vec<u8>`. See [`Options::bytes_fields`].
    #[must_use]
    pub fn bytes_field(mut self, path: impl Into<String>) -> Self {
        self.options.bytes_fields.push(path.into());
        self
    }

    /// Emit the generated service trait's response position as
    /// `impl IntoResponseBytes` instead of the concrete owned message type
    /// (default: false). Handlers can then return either the typed message
    /// or [`connectrpc::PreEncoded`]. See [`Options::generic_response_type`].
    #[must_use]
    pub fn generic_response_type(mut self, enabled: bool) -> Self {
        self.options.generic_response_type = enabled;
        self
    }

    /// Invoke `buf build` instead of `protoc`.
    ///
    /// Requires `buf` on PATH. Uses buf's dependency resolution (BSR modules)
    /// and `buf.yaml` configuration; [`Config::includes`] is ignored. When
    /// using buf, [`Config::files`] must contain proto-relative names as
    /// they appear in the buf module (e.g. `"my/service.proto"`), not
    /// filesystem paths.
    #[must_use]
    pub fn use_buf(mut self) -> Self {
        self.descriptor_source = DescriptorSource::Buf;
        self
    }

    /// Read a precompiled `FileDescriptorSet` from disk instead of invoking
    /// a compiler.
    ///
    /// Produce the file once with `protoc --descriptor_set_out=... --include_imports`
    /// or `buf build --as-file-descriptor-set -o ...`, then ship it with
    /// your source.
    ///
    /// [`Config::files`] selects which files in the set to generate code for.
    /// **These must be the proto-relative names as they appear in the
    /// descriptor set** (e.g. `"my/service.proto"`), not filesystem paths.
    /// See the `.proto` file's `name` field in the descriptor, which protoc
    /// sets to the path relative to `--proto_path`.
    #[must_use]
    pub fn descriptor_set(mut self, path: impl Into<PathBuf>) -> Self {
        self.descriptor_source = DescriptorSource::Precompiled(path.into());
        self
    }

    /// Emit an `include!`-based module tree file alongside the per-file
    /// `.rs` outputs.
    ///
    /// The file contains nested `pub mod` blocks matching the proto package
    /// hierarchy, each `include!`-ing the relevant generated file. Include
    /// it from your crate root:
    ///
    /// ```rust,ignore
    /// include!(concat!(env!("OUT_DIR"), "/_connectrpc.rs"));
    /// ```
    #[must_use]
    pub fn include_file(mut self, name: impl Into<String>) -> Self {
        self.include_file = Some(name.into());
        self
    }

    /// Run code generation and write output files.
    ///
    /// # Errors
    ///
    /// - `$OUT_DIR` is unset and no `out_dir` was configured
    /// - `protoc` or `buf` is not on `PATH` (when using those sources)
    /// - the compiler exits non-zero (syntax error, missing import, ...)
    /// - a precompiled descriptor set cannot be read or decoded
    /// - codegen fails (unsupported proto feature)
    /// - the output directory cannot be created or written to
    pub fn compile(self) -> Result<()> {
        // When out_dir() is explicitly set, emit sibling-relative include!
        // paths — the include file lives next to the generated files and
        // is referenced as a module. When defaulted from $OUT_DIR, emit
        // the env!("OUT_DIR") form for the build.rs/include! workflow.
        let relative_includes = self.out_dir.is_some();
        let out_dir = match self.out_dir {
            Some(d) => d,
            None => std::env::var_os("OUT_DIR")
                .map(PathBuf::from)
                .context("OUT_DIR is not set and no out_dir() was configured")?,
        };

        // 1. Acquire descriptor bytes and resolve files_to_generate.
        //
        // `FileDescriptorProto.name` is the path relative to the include
        // directory, not the filesystem path. For the Protoc mode we strip
        // the longest matching include prefix to recover this name. For
        // Buf and Precompiled modes, the user must already provide
        // proto-relative names in .files() (see docs on use_buf() and
        // descriptor_set()) so we pass them through as-is.
        let (descriptor_bytes, files_to_generate) = match &self.descriptor_source {
            DescriptorSource::Protoc => {
                let bytes = run_protoc(&self.files, &self.includes)?;
                // Sort includes longest-first so nested prefixes like
                // ["proto/", "proto/vendor/"] strip the most specific one.
                let mut includes = self.includes.clone();
                includes.sort_by_key(|p| std::cmp::Reverse(p.as_os_str().len()));
                let files = self
                    .files
                    .iter()
                    .map(|f| strip_include_prefix(f, &includes))
                    .filter(|s| !s.is_empty())
                    .collect();
                (bytes, files)
            }
            DescriptorSource::Buf => {
                let bytes = run_buf(&self.files)?;
                (bytes, proto_relative_names(&self.files))
            }
            DescriptorSource::Precompiled(p) => {
                let bytes = std::fs::read(p)
                    .with_context(|| format!("failed to read descriptor set '{}'", p.display()))?;
                (bytes, proto_relative_names(&self.files))
            }
        };
        let fds = FileDescriptorSet::decode_from_slice(&descriptor_bytes)
            .map_err(|e| anyhow!("failed to decode FileDescriptorSet: {e}"))?;

        // 3. Generate.
        let generated = codegen::generate_files(&fds.file, &files_to_generate, &self.options)?;

        // 4. Write per-file outputs and collect (name, package) pairs.
        std::fs::create_dir_all(&out_dir)
            .with_context(|| format!("failed to create out_dir '{}'", out_dir.display()))?;

        // Pre-build a lookup from generated filename → proto package.
        let name_to_package: std::collections::HashMap<String, String> = fds
            .file
            .iter()
            .filter_map(|fd| {
                let proto_name = fd.name.as_deref()?;
                let rs_name = buffa_codegen::proto_path_to_rust_module(proto_name);
                Some((rs_name, fd.package.clone().unwrap_or_default()))
            })
            .collect();

        let mut entries: Vec<(String, String)> = Vec::with_capacity(generated.len());
        for file in &generated {
            let path = out_dir.join(&file.name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            write_if_changed(&path, file.content.as_bytes())?;
            let pkg = name_to_package.get(&file.name).cloned().unwrap_or_default();
            entries.push((file.name.clone(), pkg));
        }

        // 5. Optionally emit the module-tree include file.
        if let Some(ref include_name) = self.include_file {
            let include_src = generate_include_file(&entries, relative_includes);
            let include_path = out_dir.join(include_name);
            write_if_changed(&include_path, include_src.as_bytes())?;
        }

        // 6. Cargo re-run triggers. In Precompiled mode `self.files` holds
        // proto-relative names (per the docs on `descriptor_set()`), not
        // on-disk paths; emitting `rerun-if-changed` for them points cargo at
        // missing files and forces a rebuild on every invocation. The `.pb`
        // path is the only real input in that mode.
        match &self.descriptor_source {
            DescriptorSource::Precompiled(p) => {
                println!("cargo:rerun-if-changed={}", p.display());
            }
            DescriptorSource::Protoc | DescriptorSource::Buf => {
                for f in &self.files {
                    println!("cargo:rerun-if-changed={}", f.display());
                }
            }
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// Write `content` to `path` only if the file doesn't already exist with
/// identical content. Cargo's rebuild decision for `include!`-ed files is
/// mtime-based, so an unconditional write here would cascade into
/// recompiling every downstream crate whenever any `.proto` is touched.
fn write_if_changed(path: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Ok(existing) = std::fs::read(path)
        && existing == content
    {
        return Ok(());
    }
    std::fs::write(path, content)
}

/// Run `protoc` and return the serialized `FileDescriptorSet`.
fn run_protoc(files: &[PathBuf], includes: &[PathBuf]) -> Result<Vec<u8>> {
    let protoc = std::env::var("PROTOC").unwrap_or_else(|_| "protoc".to_string());

    let out = tempfile::NamedTempFile::new().context("failed to create tempfile for protoc")?;
    let out_path = out.path().to_path_buf();

    let mut cmd = Command::new(&protoc);
    cmd.arg("--include_imports");
    cmd.arg(format!("--descriptor_set_out={}", out_path.display()));
    for inc in includes {
        cmd.arg(format!("--proto_path={}", inc.display()));
    }
    for f in files {
        cmd.arg(f.as_os_str());
    }

    let output = cmd
        .output()
        .with_context(|| format!("failed to spawn protoc ('{protoc}')"))?;
    if !output.status.success() {
        bail!("protoc failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    std::fs::read(&out_path).context("failed to read protoc descriptor output")
}

/// Run `buf build --as-file-descriptor-set` and return the serialized bytes.
///
/// Includes are intentionally NOT passed: buf's `--path` flag is a file
/// filter, not an import path like protoc's `--proto_path`. Passing include
/// directories as `--path` would restrict the output in unintended ways.
/// buf resolves imports via `buf.yaml`.
fn run_buf(files: &[PathBuf]) -> Result<Vec<u8>> {
    let out = tempfile::NamedTempFile::new().context("failed to create tempfile for buf")?;
    let out_path = out.path().to_path_buf();

    let mut cmd = Command::new("buf");
    cmd.arg("build")
        .arg("--as-file-descriptor-set")
        .arg("-o")
        .arg(&out_path);
    for f in files {
        cmd.arg("--path").arg(f.as_os_str());
    }

    let output = cmd.output().context("failed to spawn buf")?;
    if !output.status.success() {
        bail!(
            "buf build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    std::fs::read(&out_path).context("failed to read buf descriptor output")
}

/// Strip the longest matching include prefix from a filesystem path to
/// recover the proto-relative name protoc stores in `FileDescriptorProto.name`.
///
/// Falls back to the bare file name if no prefix matches. Callers must
/// pre-sort `includes` longest-first so nested include directories are
/// matched correctly.
fn strip_include_prefix(f: &Path, includes: &[PathBuf]) -> String {
    for inc in includes {
        if let Ok(rel) = f.strip_prefix(inc)
            && let Some(s) = rel.to_str()
        {
            return s.to_string();
        }
    }
    f.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Convert `.files()` paths to strings verbatim for Buf/Precompiled modes
/// where the user supplies proto-relative names directly (no include prefix
/// to strip).
fn proto_relative_names(files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .filter_map(|f| f.to_str().map(str::to_string))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Build an `include!`-based module-tree file.
///
/// Given `[("my.pkg.thing.rs", "my.pkg"), ...]`, produce nested
/// `pub mod my { pub mod pkg { include!(...); } }`.
///
/// When `relative` is false (the `$OUT_DIR` workflow), emit
/// `include!(concat!(env!("OUT_DIR"), "/my.pkg.thing.rs"))`.
/// When `relative` is true (explicit `out_dir()`), the include file and the
/// generated files are siblings, so emit `include!("my.pkg.thing.rs")` —
/// `include!` resolves relative to the including file.
fn generate_include_file(entries: &[(String, String)], relative: bool) -> String {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;

    #[derive(Default)]
    struct Node {
        files: Vec<String>,
        children: BTreeMap<String, Node>,
    }

    let mut root = Node::default();
    for (file_name, package) in entries {
        let mut node = &mut root;
        if !package.is_empty() {
            for seg in package.split('.') {
                node = node.children.entry(seg.to_string()).or_default();
            }
        }
        node.files.push(file_name.clone());
    }

    fn emit(out: &mut String, node: &Node, depth: usize, relative: bool) {
        let indent = "    ".repeat(depth);
        for f in &node.files {
            if relative {
                writeln!(out, r#"{indent}include!("{f}");"#).unwrap();
            } else {
                writeln!(
                    out,
                    r#"{indent}include!(concat!(env!("OUT_DIR"), "/{f}"));"#
                )
                .unwrap();
            }
        }
        for (name, child) in &node.children {
            let ident = buffa_codegen::idents::escape_mod_ident(name);
            writeln!(
                out,
                "{indent}#[allow(dead_code, non_camel_case_types, unused_imports, \
                 clippy::derivable_impls, clippy::match_single_binding)]"
            )
            .unwrap();
            writeln!(out, "{indent}pub mod {ident} {{").unwrap();
            writeln!(out, "{indent}    use super::*;").unwrap();
            emit(out, child, depth + 1, relative);
            writeln!(out, "{indent}}}").unwrap();
        }
    }

    let mut out = String::new();
    writeln!(out, "// @generated by connectrpc-build. DO NOT EDIT.").unwrap();
    writeln!(out).unwrap();
    emit(&mut out, &root, 0, relative);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_file_nests_packages() {
        let entries = vec![
            ("my.pkg.svc.rs".into(), "my.pkg".into()),
            ("my.other.rs".into(), "my".into()),
            ("root.rs".into(), String::new()),
        ];
        let out = generate_include_file(&entries, false);

        assert!(
            out.contains("// @generated by connectrpc-build"),
            "missing header: {out}"
        );
        // Root-level file has no wrapper.
        assert!(
            out.contains(r#"include!(concat!(env!("OUT_DIR"), "/root.rs"));"#),
            "missing root include: {out}"
        );
        // my.pkg.svc.rs is nested two levels deep.
        assert!(out.contains("pub mod my {"), "missing mod my: {out}");
        assert!(out.contains("pub mod pkg {"), "missing mod pkg: {out}");
        assert!(
            out.contains(r#"include!(concat!(env!("OUT_DIR"), "/my.pkg.svc.rs"));"#),
            "missing nested include: {out}"
        );
        // my.other.rs is one level deep (under mod my).
        assert!(
            out.contains(r#"include!(concat!(env!("OUT_DIR"), "/my.other.rs"));"#),
            "missing my.other include: {out}"
        );
    }

    #[test]
    fn include_file_relative_mode() {
        let entries = vec![
            ("my.pkg.svc.rs".into(), "my.pkg".into()),
            ("root.rs".into(), String::new()),
        ];
        let out = generate_include_file(&entries, true);

        // Relative mode uses bare sibling paths, no env!/concat!.
        assert!(
            out.contains(r#"include!("root.rs");"#),
            "missing relative root include: {out}"
        );
        assert!(
            out.contains(r#"include!("my.pkg.svc.rs");"#),
            "missing relative nested include: {out}"
        );
        assert!(
            !out.contains("env!"),
            "relative mode should not emit env!: {out}"
        );
        assert!(
            !out.contains("concat!"),
            "relative mode should not emit concat!: {out}"
        );
        // Module tree is the same regardless of include form.
        assert!(out.contains("pub mod my {"), "missing mod my: {out}");
        assert!(out.contains("pub mod pkg {"), "missing mod pkg: {out}");
    }

    #[test]
    fn include_file_escapes_keywords() {
        let entries = vec![("type.match.svc.rs".into(), "type.match".into())];
        let out = generate_include_file(&entries, false);
        assert!(out.contains("pub mod r#type {"), "expected r#type: {out}");
        assert!(out.contains("pub mod r#match {"), "expected r#match: {out}");
    }

    #[test]
    fn config_builder_chain() {
        let cfg = Config::new()
            .files(&["a.proto", "b.proto"])
            .includes(&["proto/"])
            .strict_utf8_mapping(true)
            .generate_json(false)
            .emit_register_fn(false)
            .include_file("_inc.rs");
        assert_eq!(cfg.files.len(), 2);
        assert_eq!(cfg.includes.len(), 1);
        assert!(cfg.options.strict_utf8_mapping);
        assert!(!cfg.options.generate_json);
        assert!(!cfg.options.emit_register_fn);
        assert_eq!(cfg.include_file.as_deref(), Some("_inc.rs"));
    }

    #[test]
    fn config_default_options() {
        let cfg = Config::new();
        assert!(!cfg.options.strict_utf8_mapping);
        assert!(cfg.options.generate_json);
        assert!(cfg.options.emit_register_fn);
        assert!(matches!(cfg.descriptor_source, DescriptorSource::Protoc));
    }

    #[test]
    fn config_descriptor_source_variants() {
        assert!(matches!(
            Config::new().use_buf().descriptor_source,
            DescriptorSource::Buf
        ));
        assert!(matches!(
            Config::new().descriptor_set("x.bin").descriptor_source,
            DescriptorSource::Precompiled(_)
        ));
    }

    /// End-to-end: precompiled descriptor set → generated Rust in a tempdir.
    /// Verifies the file layout and that the service binding imports use
    /// `::connectrpc::` (absolute path).
    #[test]
    fn compile_precompiled_descriptor_set() {
        let fixture = format!("{}/tests/fixtures/echo.fds.bin", env!("CARGO_MANIFEST_DIR"));
        let out = tempfile::tempdir().unwrap();

        Config::new()
            .descriptor_set(&fixture)
            .files(&["echo.proto"])
            .out_dir(out.path())
            .include_file("_inc.rs")
            .compile()
            .unwrap();

        // Per-file output.
        let echo_rs = out.path().join("echo.rs");
        assert!(echo_rs.exists(), "expected {echo_rs:?} to exist");
        let content = std::fs::read_to_string(&echo_rs).unwrap();

        // Message types from buffa-codegen.
        assert!(content.contains("pub struct EchoRequest"));
        assert!(content.contains("pub struct EchoResponse"));
        // Service trait + client from connectrpc-codegen.
        assert!(content.contains("pub trait EchoService"));
        assert!(content.contains("pub struct EchoServiceClient"));
        // Fully qualified paths (the module-collision fix): no top-level `use`
        // statements, all references are inline absolute paths like
        // `::connectrpc::Context`, `::std::sync::Arc`, etc.
        assert!(
            content.contains("::connectrpc::"),
            "service code should use ::connectrpc:: fully qualified paths"
        );
        assert!(
            !content.contains("\nuse "),
            "service code should not emit top-level use statements"
        );

        // Include file nests under test.echo.v1. Because out_dir() was set
        // explicitly (not defaulted from $OUT_DIR), includes are sibling-
        // relative.
        let inc = std::fs::read_to_string(out.path().join("_inc.rs")).unwrap();
        assert!(inc.contains("pub mod test {"));
        assert!(inc.contains("pub mod echo {"));
        assert!(inc.contains("pub mod v1 {"));
        assert!(inc.contains(r#"include!("echo.rs");"#));
    }

    #[test]
    fn compile_rejects_unknown_file_names() {
        let fixture = format!("{}/tests/fixtures/echo.fds.bin", env!("CARGO_MANIFEST_DIR"));
        let out = tempfile::tempdir().unwrap();

        let err = Config::new()
            .descriptor_set(&fixture)
            .files(&["nonexistent.proto"])
            .out_dir(out.path())
            .compile()
            .unwrap_err();

        // buffa-codegen rejects unknown file_to_generate entries; verify that
        // error surfaces through compile() with the offending name.
        let msg = err.to_string();
        assert!(
            msg.contains("nonexistent.proto"),
            "error should name the missing file: {msg}"
        );
    }

    /// descriptor_set() with nested proto paths must preserve directory
    /// components — users provide proto-relative names like
    /// "my/pkg/ping.proto" which match what's inside the FDS. Stripping
    /// to file_name() would fail to find the descriptor.
    #[test]
    fn compile_precompiled_preserves_nested_paths() {
        let fixture = format!(
            "{}/tests/fixtures/nested.fds.bin",
            env!("CARGO_MANIFEST_DIR")
        );
        let out = tempfile::tempdir().unwrap();

        Config::new()
            .descriptor_set(&fixture)
            // nested path with directory components — before the fix,
            // this was stripped to "ping.proto" and failed to match
            .files(&["my/pkg/ping.proto"])
            .out_dir(out.path())
            .include_file("_inc.rs")
            .compile()
            .unwrap();

        // Output filename derived from proto path with dots.
        let out_rs = out.path().join("my.pkg.ping.rs");
        assert!(out_rs.exists(), "expected {out_rs:?}");
        let content = std::fs::read_to_string(&out_rs).unwrap();
        assert!(content.contains("pub struct PingRequest"));
        assert!(content.contains("pub trait PingService"));

        // Include file nests under my.pkg.v1.
        let inc = std::fs::read_to_string(out.path().join("_inc.rs")).unwrap();
        assert!(inc.contains("pub mod my {"));
        assert!(inc.contains("pub mod pkg {"));
        assert!(inc.contains("pub mod v1 {"));
    }

    #[test]
    fn strip_include_prefix_longest_first() {
        // With overlapping includes, the longest must win.
        let includes = vec![PathBuf::from("proto/vendor/"), PathBuf::from("proto/")];
        // Caller contract: sorted longest-first.
        let mut sorted = includes.clone();
        sorted.sort_by_key(|p| std::cmp::Reverse(p.as_os_str().len()));
        assert_eq!(sorted[0], PathBuf::from("proto/vendor/"));

        let f = PathBuf::from("proto/vendor/thing.proto");
        assert_eq!(strip_include_prefix(&f, &sorted), "thing.proto");

        let f = PathBuf::from("proto/my/svc.proto");
        assert_eq!(strip_include_prefix(&f, &sorted), "my/svc.proto");
    }

    #[test]
    fn strip_include_prefix_fallback_to_filename() {
        let f = PathBuf::from("unrelated/path/svc.proto");
        let includes = vec![PathBuf::from("proto/")];
        assert_eq!(strip_include_prefix(&f, &includes), "svc.proto");
    }

    #[test]
    fn proto_relative_names_verbatim() {
        let files = vec![
            PathBuf::from("my/pkg/svc.proto"),
            PathBuf::from("top.proto"),
        ];
        assert_eq!(
            proto_relative_names(&files),
            vec!["my/pkg/svc.proto".to_string(), "top.proto".to_string()]
        );
    }

    #[test]
    fn write_if_changed_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.rs");
        write_if_changed(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn write_if_changed_skips_identical_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("same.rs");
        std::fs::write(&path, b"content").unwrap();
        let mtime_before = std::fs::metadata(&path).unwrap().modified().unwrap();

        // Sleep briefly so a write would produce a distinguishable mtime.
        std::thread::sleep(std::time::Duration::from_millis(50));

        write_if_changed(&path, b"content").unwrap();
        let mtime_after = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(mtime_before, mtime_after);
    }

    #[test]
    fn write_if_changed_overwrites_different_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("changed.rs");
        std::fs::write(&path, b"old").unwrap();

        write_if_changed(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }
}
