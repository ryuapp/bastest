use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, CallExpression, ExportAllDeclaration, ExportNamedDeclaration, Expression,
    ImportDeclaration, ImportDeclarationSpecifier, ImportExpression, ImportOrExportKind,
    StringLiteral,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::{ParseOptions, Parser};
use oxc_semantic::{IsGlobalReference, Scoping, SemanticBuilder};
use oxc_span::{GetSpan, SourceType, Span};
use rolldown::plugin::{
    HookTransformArgs, HookTransformOutput, HookTransformReturn, HookUsage, Plugin,
    SharedTransformPluginContext,
};
use rolldown::{
    Bundler, BundlerOptions, CodeSplittingMode, Either, EnhancedTransformOptions, InputItem,
    JsxOptions, OutputFormat, Platform, PreserveEntrySignatures, TsconfigOption, TypeScriptOptions,
    enhanced_transform,
};
use tokio::runtime::Runtime;

use crate::cache_version::DEPENDENCY_CACHE_VERSION;

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum ModuleFormat {
    Esm,
    Cjs,
}

impl ModuleFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Esm => "mjs",
            Self::Cjs => "cjs",
        }
    }

    fn rolldown_format(self) -> OutputFormat {
        match self {
            Self::Esm => OutputFormat::Esm,
            Self::Cjs => OutputFormat::Cjs,
        }
    }
}

pub struct TransformContext {
    dependencies: HashMap<ModuleFormat, Arc<HashMap<String, String>>>,
}

#[derive(Default)]
struct DependencyDiscovery {
    visited: HashSet<PathBuf>,
    specifiers: HashSet<String>,
}

impl TransformContext {
    fn dependencies(&self, module_format: ModuleFormat) -> Arc<HashMap<String, String>> {
        self.dependencies
            .get(&module_format)
            .cloned()
            .unwrap_or_else(|| Arc::new(HashMap::new()))
    }
}

pub fn prepare_test_run(
    files: &[PathBuf],
    project_root: &Path,
) -> Result<TransformContext, String> {
    let mut discovery_by_format: HashMap<ModuleFormat, DependencyDiscovery> = HashMap::new();
    for file in files {
        let module_format = detect_module_format(file)?;
        let discovery = discovery_by_format.entry(module_format).or_default();
        collect_reachable_dependencies(file, &mut discovery.visited, &mut discovery.specifiers)?;
    }

    let mut dependencies = HashMap::new();
    for (module_format, discovery) in discovery_by_format {
        dependencies.insert(
            module_format,
            optimize_dependencies(project_root, discovery.specifiers, module_format)?,
        );
    }
    Ok(TransformContext { dependencies })
}

pub fn transform_test_file(
    file: &Path,
    package_root: &Path,
    project_root: &Path,
    context: &TransformContext,
) -> Result<PathBuf, String> {
    let extension = file
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let output_extension = if matches!(extension, "jsx" | "tsx") || extension.is_empty() {
        "js"
    } else {
        extension
    };
    let source = fs::read_to_string(file)
        .map_err(|error| format!("failed to read {}: {error}", file.display()))?;
    let module_format = detect_module_format(file)?;
    let dependencies = context.dependencies(module_format);
    let run_dir = project_root.join(".bastest").join("run").join(format!(
        "{}-{}",
        std::process::id(),
        RUN_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let entry = run_dir.join(format!("test.entry.{output_extension}"));
    let output = run_dir.join(format!("test.bundle.{}", module_format.extension()));
    fs::create_dir_all(&run_dir).map_err(|error| {
        format!(
            "failed to create test bundle directory {}: {error}",
            run_dir.display()
        )
    })?;

    let rewritten = rewrite_assert_calls(
        file,
        &rewrite_module_specifiers(file, &rewrite_import_meta_test(&source), package_root)?,
    )?;
    let code = if matches!(extension, "jsx" | "tsx") {
        transform_jsx(file, &rewritten)?
    } else {
        rewritten
    };

    fs::write(&entry, code).map_err(|error| {
        format!(
            "failed to write transformed test {}: {error}",
            entry.display()
        )
    })?;
    bundle_test_file(&entry, &output, project_root, module_format, dependencies)?;
    Ok(output)
}

fn detect_module_format(file: &Path) -> Result<ModuleFormat, String> {
    let Some(parent) = file.parent() else {
        return Ok(ModuleFormat::Esm);
    };
    for directory in parent.ancestors() {
        let manifest_path = directory.join("package.json");
        if !manifest_path.is_file() {
            continue;
        }
        let manifest = fs::read_to_string(&manifest_path)
            .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
        let manifest: serde_json::Value = serde_json::from_str(&manifest)
            .map_err(|error| format!("failed to parse {}: {error}", manifest_path.display()))?;
        return Ok(
            if manifest.get("type").and_then(|value| value.as_str()) == Some("module") {
                ModuleFormat::Esm
            } else {
                ModuleFormat::Cjs
            },
        );
    }
    Ok(ModuleFormat::Esm)
}

fn bundle_test_file(
    entry: &Path,
    output: &Path,
    project_root: &Path,
    module_format: ModuleFormat,
    dependencies: Arc<HashMap<String, String>>,
) -> Result<(), String> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create bundle directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let options = BundlerOptions {
        input: Some(vec![InputItem {
            name: Some("test".to_string()),
            import: entry.to_string_lossy().to_string(),
        }]),
        cwd: Some(project_root.to_path_buf()),
        external: Some(dependencies.values().cloned().collect::<Vec<_>>().into()),
        file: Some(output.to_string_lossy().to_string()),
        format: Some(module_format.rolldown_format()),
        platform: Some(Platform::Node),
        sourcemap: None,
        ..Default::default()
    };
    let mut bundler =
        Bundler::with_plugins(options, vec![Arc::new(RuntimePathPlugin { dependencies })])
            .map_err(|errors| format!("failed to create test bundler: {errors:?}"))?;

    rolldown_runtime()
        .block_on(bundler.write())
        .map_err(|errors| format!("failed to bundle {}: {errors:?}", entry.display()))?;
    Ok(())
}

#[derive(Debug)]
struct RuntimePathPlugin {
    dependencies: Arc<HashMap<String, String>>,
}

impl Plugin for RuntimePathPlugin {
    fn name(&self) -> Cow<'static, str> {
        "bastest-runtime-paths".into()
    }

    async fn transform(
        &self,
        _ctx: SharedTransformPluginContext,
        args: &HookTransformArgs<'_>,
    ) -> HookTransformReturn {
        let path = Path::new(args.id);
        if !path.is_absolute() || !path.is_file() {
            return Ok(None);
        }
        let Some(code) = rewrite_bundled_module(path, args.code, &self.dependencies) else {
            return Ok(None);
        };
        Ok(Some(HookTransformOutput {
            code: Some(code),
            ..Default::default()
        }))
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::Transform
    }
}

fn rewrite_bundled_module(
    file: &Path,
    source: &str,
    dependencies: &HashMap<String, String>,
) -> Option<String> {
    let (mut replacements, globals) = collect_runtime_replacements(file, source, dependencies);
    let uses_filename = globals.filename;
    let uses_dirname = globals.dirname;
    if !uses_filename && !uses_dirname && replacements.is_empty() {
        return None;
    }

    let filename = absolute_module_specifier(file).ok()?;
    let dirname = absolute_module_specifier(file.parent()?).ok()?;
    let mut prefix = String::new();
    if globals.insertion > 0 {
        prefix.push('\n');
    }
    if uses_filename {
        prefix.push_str(&format!(
            "var __filename = {};\n",
            serde_json::to_string(&filename).unwrap()
        ));
    }
    if uses_dirname {
        prefix.push_str(&format!(
            "var __dirname = {};\n",
            serde_json::to_string(&dirname).unwrap()
        ));
    }
    replacements.push(SpanReplacement {
        start: globals.insertion,
        end: globals.insertion,
        text: prefix,
    });
    Some(apply_span_replacements(source, &mut replacements))
}

fn collect_runtime_replacements(
    file: &Path,
    source: &str,
    dependencies: &HashMap<String, String>,
) -> (Vec<SpanReplacement>, RuntimeGlobals) {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(file).unwrap_or_else(|_| SourceType::mjs());
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return (Vec::new(), RuntimeGlobals::default());
    }
    let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
    let unresolved = semantic.scoping().root_unresolved_references();
    let globals = RuntimeGlobals {
        filename: unresolved.contains_key("__filename"),
        dirname: unresolved.contains_key("__dirname"),
        insertion: runtime_global_insertion(&parsed.program, source),
    };
    let mut collector = RequireResolveCollector::new(semantic.scoping());
    collector.visit_program(&parsed.program);
    let parent = file.parent().unwrap_or_else(|| Path::new("."));
    let mut replacements = collector
        .specifiers
        .into_iter()
        .filter_map(|specifier| {
            is_relative_specifier(&specifier.value)
                .then(|| absolute_module_specifier(&parent.join(&specifier.value)).ok())
                .flatten()
                .map(|replacement| SpanReplacement {
                    start: specifier.span.start as usize,
                    end: specifier.span.end as usize,
                    text: serde_json::to_string(&replacement).unwrap(),
                })
        })
        .collect::<Vec<_>>();
    if let Ok(file_url) = url::Url::from_file_path(file) {
        replacements.extend(
            collector
                .import_meta_urls
                .into_iter()
                .map(|span| SpanReplacement {
                    start: span.start as usize,
                    end: span.end as usize,
                    text: serde_json::to_string(file_url.as_str()).unwrap(),
                }),
        );
    }
    replacements.extend(collector.modules.into_iter().filter_map(|specifier| {
        dependencies
            .get(&specifier.value)
            .map(|replacement| SpanReplacement {
                start: specifier.span.start as usize,
                end: specifier.span.end as usize,
                text: serde_json::to_string(replacement).unwrap(),
            })
    }));
    (replacements, globals)
}

#[derive(Default)]
struct RuntimeGlobals {
    filename: bool,
    dirname: bool,
    insertion: usize,
}

fn runtime_global_insertion(program: &oxc_ast::ast::Program<'_>, source: &str) -> usize {
    let end = program
        .directives
        .last()
        .map(|directive| directive.span.end as usize)
        .or_else(|| {
            program
                .hashbang
                .as_ref()
                .map(|hashbang| hashbang.span.end as usize)
        })
        .unwrap_or(0);
    if source.as_bytes().get(end) == Some(&b';') {
        end + 1
    } else {
        end
    }
}

struct RequireResolveCollector<'s> {
    scoping: &'s Scoping,
    specifiers: Vec<ModuleSpecifier>,
    import_meta_urls: Vec<Span>,
    modules: Vec<ModuleSpecifier>,
}

impl<'s> RequireResolveCollector<'s> {
    fn new(scoping: &'s Scoping) -> Self {
        Self {
            scoping,
            specifiers: Vec::new(),
            import_meta_urls: Vec::new(),
            modules: Vec::new(),
        }
    }
}

impl<'a> Visit<'a> for RequireResolveCollector<'_> {
    fn visit_import_declaration(&mut self, declaration: &ImportDeclaration<'a>) {
        self.modules.push(ModuleSpecifier {
            span: declaration.source.span,
            value: declaration.source.value.to_string(),
        });
        walk::walk_import_declaration(self, declaration);
    }

    fn visit_export_named_declaration(&mut self, declaration: &ExportNamedDeclaration<'a>) {
        if let Some(source) = &declaration.source {
            self.modules.push(ModuleSpecifier {
                span: source.span,
                value: source.value.to_string(),
            });
        }
        walk::walk_export_named_declaration(self, declaration);
    }

    fn visit_export_all_declaration(&mut self, declaration: &ExportAllDeclaration<'a>) {
        self.modules.push(ModuleSpecifier {
            span: declaration.source.span,
            value: declaration.source.value.to_string(),
        });
        walk::walk_export_all_declaration(self, declaration);
    }

    fn visit_import_expression(&mut self, expression: &ImportExpression<'a>) {
        if let Expression::StringLiteral(source) = &expression.source {
            self.modules.push(ModuleSpecifier {
                span: source.span,
                value: source.value.to_string(),
            });
        }
        walk::walk_import_expression(self, expression);
    }

    fn visit_static_member_expression(
        &mut self,
        member: &oxc_ast::ast::StaticMemberExpression<'a>,
    ) {
        if matches!(
            &member.object,
            Expression::MetaProperty(meta)
                if meta.meta.name == "import"
                    && meta.property.name == "meta"
                    && member.property.name == "url"
        ) {
            self.import_meta_urls.push(member.span);
        }
        walk::walk_static_member_expression(self, member);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        let is_require_resolve = matches!(
            &call.callee,
            Expression::StaticMemberExpression(member)
                if matches!(&member.object, Expression::Identifier(identifier) if identifier.is_global_reference_name("require".into(), self.scoping))
                    && member.property.name == "resolve"
        );
        if is_require_resolve && let Some(Argument::StringLiteral(source)) = call.arguments.first()
        {
            self.specifiers.push(ModuleSpecifier {
                span: source.span,
                value: source.value.to_string(),
            });
        }
        if matches!(&call.callee, Expression::Identifier(identifier) if identifier.is_global_reference_name("require".into(), self.scoping))
            && let Some(Argument::StringLiteral(source)) = call.arguments.first()
        {
            self.modules.push(ModuleSpecifier {
                span: source.span,
                value: source.value.to_string(),
            });
        }
        walk::walk_call_expression(self, call);
    }
}

fn optimize_dependencies(
    project_root: &Path,
    specifiers: HashSet<String>,
    module_format: ModuleFormat,
) -> Result<Arc<HashMap<String, String>>, String> {
    let mut specifiers = specifiers.into_iter().collect::<Vec<_>>();
    specifiers.sort_unstable();
    let outputs = optimize_dependency_set(project_root, &specifiers, module_format)?;
    let mut optimized = HashMap::new();
    for (specifier, output) in specifiers.into_iter().zip(outputs) {
        let module = match module_format {
            ModuleFormat::Esm => url::Url::from_file_path(&output)
                .map_err(|_| format!("failed to create file URL for {}", output.display()))?
                .to_string(),
            ModuleFormat::Cjs => absolute_external_specifier(&output)?,
        };
        optimized.insert(specifier, module);
    }
    Ok(Arc::new(optimized))
}

fn collect_reachable_dependencies(
    file: &Path,
    visited: &mut HashSet<PathBuf>,
    specifiers: &mut HashSet<String>,
) -> Result<(), String> {
    let file = std::path::absolute(file)
        .map_err(|error| format!("failed to resolve {}: {error}", file.display()))?;
    if !visited.insert(file.clone()) {
        return Ok(());
    }
    let source = fs::read_to_string(&file)
        .map_err(|error| format!("failed to read {}: {error}", file.display()))?;
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(&file).unwrap_or_else(|_| SourceType::mjs());
    let parsed = Parser::new(&allocator, &source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return Ok(());
    }
    let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
    let mut collector = ModuleSpecifierCollector::new(semantic.scoping());
    collector.visit_program(&parsed.program);
    let parent = file.parent().unwrap_or_else(|| Path::new("."));
    for module in collector.specifiers {
        if is_bare_dependency(&module.value) {
            specifiers.insert(module.value);
        } else if is_relative_specifier(&module.value)
            && let Some(local) = resolve_local_module(parent, &module.value)
        {
            collect_reachable_dependencies(&local, visited, specifiers)?;
        }
    }
    Ok(())
}

fn resolve_local_module(parent: &Path, specifier: &str) -> Option<PathBuf> {
    let candidate = parent.join(specifier);
    if candidate.is_file() {
        return Some(candidate);
    }
    const EXTENSIONS: &[&str] = &["js", "mjs", "cjs", "jsx", "ts", "mts", "cts", "tsx"];
    for extension in EXTENSIONS {
        let with_extension = candidate.with_extension(extension);
        if with_extension.is_file() {
            return Some(with_extension);
        }
    }
    for extension in EXTENSIONS {
        let index = candidate.join(format!("index.{extension}"));
        if index.is_file() {
            return Some(index);
        }
    }
    None
}

#[derive(Debug, Eq, PartialEq)]
enum ModuleSpecifierKind {
    BastestRuntime,
    NodeBuiltin,
    RelativePath,
    AbsolutePath,
    Url,
    PackageImport,
    BareDependency,
    Invalid,
}

fn classify_module_specifier(specifier: &str) -> ModuleSpecifierKind {
    if specifier.is_empty() {
        return ModuleSpecifierKind::Invalid;
    }
    if specifier == "bastest" {
        return ModuleSpecifierKind::BastestRuntime;
    }
    if is_node_builtin(specifier) {
        return ModuleSpecifierKind::NodeBuiltin;
    }
    if is_relative_specifier(specifier) {
        return ModuleSpecifierKind::RelativePath;
    }
    if specifier.starts_with('#') {
        return ModuleSpecifierKind::PackageImport;
    }
    if Path::new(specifier).is_absolute()
        || specifier.starts_with('/')
        || specifier.starts_with('\\')
    {
        return ModuleSpecifierKind::AbsolutePath;
    }
    if url::Url::parse(specifier).is_ok() {
        return ModuleSpecifierKind::Url;
    }
    if is_package_specifier(specifier) {
        ModuleSpecifierKind::BareDependency
    } else {
        ModuleSpecifierKind::Invalid
    }
}

fn is_package_specifier(specifier: &str) -> bool {
    let mut segments = specifier.split('/');
    let Some(first) = segments.next() else {
        return false;
    };
    if let Some(scope) = first.strip_prefix('@') {
        if !is_valid_package_name(scope) {
            return false;
        }
        let Some(package) = segments.next() else {
            return false;
        };
        if !is_valid_package_name(package) {
            return false;
        }
    } else if !is_valid_package_name(first) {
        return false;
    }
    segments.all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && segment.chars().all(is_package_subpath_character)
    })
}

fn is_valid_package_name(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
        && value.chars().all(is_package_name_character)
}

fn is_package_name_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
}

fn is_package_subpath_character(character: char) -> bool {
    !character.is_control() && !matches!(character, '\\' | ':' | '?' | '#')
}

fn is_bare_dependency(specifier: &str) -> bool {
    classify_module_specifier(specifier) == ModuleSpecifierKind::BareDependency
}

fn is_node_builtin(specifier: &str) -> bool {
    if specifier.starts_with("node:") {
        return true;
    }
    let root = specifier.split('/').next().unwrap_or(specifier);
    matches!(
        root,
        "assert"
            | "async_hooks"
            | "buffer"
            | "child_process"
            | "cluster"
            | "console"
            | "constants"
            | "crypto"
            | "dgram"
            | "diagnostics_channel"
            | "dns"
            | "domain"
            | "events"
            | "fs"
            | "http"
            | "http2"
            | "https"
            | "module"
            | "net"
            | "os"
            | "path"
            | "perf_hooks"
            | "process"
            | "punycode"
            | "querystring"
            | "readline"
            | "repl"
            | "stream"
            | "string_decoder"
            | "sys"
            | "timers"
            | "tls"
            | "trace_events"
            | "tty"
            | "url"
            | "util"
            | "v8"
            | "vm"
            | "wasi"
            | "worker_threads"
            | "zlib"
    )
}

fn optimize_dependency_set(
    project_root: &Path,
    specifiers: &[String],
    module_format: ModuleFormat,
) -> Result<Vec<PathBuf>, String> {
    if specifiers.is_empty() {
        return Ok(Vec::new());
    }

    let mut hasher = DefaultHasher::new();
    DEPENDENCY_CACHE_VERSION.hash(&mut hasher);
    specifiers.hash(&mut hasher);
    module_format.extension().hash(&mut hasher);
    hash_dependency_state(project_root, &mut hasher)?;
    let key = format!("{:016x}", hasher.finish());
    let deps_dir = project_root.join(".bastest/cache/deps");
    let cache_dir = deps_dir.join(&key);
    let outputs = dependency_outputs(&cache_dir, specifiers.len(), module_format);
    if cache_dir.join(".complete").is_file() && outputs.iter().all(|output| output.is_file()) {
        return Ok(outputs);
    }
    fs::create_dir_all(&deps_dir)
        .map_err(|error| format!("failed to create {}: {error}", deps_dir.display()))?;
    let staging = deps_dir.join(format!(
        ".{key}.{}-{}.tmp",
        std::process::id(),
        RUN_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    if let Err(error) = fs::create_dir_all(&staging) {
        return Err(cleanup_staging(
            &staging,
            format!("failed to create {}: {error}", staging.display()),
        ));
    }
    if let Err(error) = build_dependency_set(project_root, &staging, specifiers, module_format) {
        return Err(cleanup_staging(&staging, error));
    }
    match fs::rename(&staging, &cache_dir) {
        Ok(()) => Ok(outputs),
        Err(_)
            if cache_dir.join(".complete").is_file()
                && outputs.iter().all(|output| output.is_file()) =>
        {
            fs::remove_dir_all(&staging).map_err(|error| {
                format!(
                    "dependency cache was published by another process, but failed to remove {}: {error}",
                    staging.display()
                )
            })?;
            Ok(outputs)
        }
        Err(error) => Err(cleanup_staging(
            &staging,
            format!(
                "failed to publish dependency cache {}: {error}",
                cache_dir.display()
            ),
        )),
    }
}

fn build_dependency_set(
    project_root: &Path,
    staging: &Path,
    specifiers: &[String],
    module_format: ModuleFormat,
) -> Result<(), String> {
    let mut inputs = Vec::with_capacity(specifiers.len());
    for (index, specifier) in specifiers.iter().enumerate() {
        let name = format!("dependency-{index}");
        let entry_proxy = staging.join(format!("entry-{index}.{}", module_format.extension()));
        let quoted = serde_json::to_string(specifier).unwrap();
        let entry_source = match module_format {
            ModuleFormat::Esm => {
                format!("export * from {quoted};\nexport {{ default }} from {quoted};\n")
            }
            ModuleFormat::Cjs => format!("module.exports = require({quoted});\n"),
        };
        fs::write(&entry_proxy, entry_source).map_err(|error| {
            format!(
                "failed to write dependency proxy {}: {error}",
                entry_proxy.display()
            )
        })?;
        inputs.push(InputItem {
            name: Some(name),
            import: entry_proxy.to_string_lossy().to_string(),
        });
    }
    let options = BundlerOptions {
        input: Some(inputs),
        cwd: Some(project_root.to_path_buf()),
        dir: Some(staging.to_string_lossy().to_string()),
        entry_filenames: Some(format!("internal-[name].{}", module_format.extension()).into()),
        chunk_filenames: Some(format!("chunk-[hash].{}", module_format.extension()).into()),
        format: Some(module_format.rolldown_format()),
        platform: Some(Platform::Node),
        code_splitting: Some(CodeSplittingMode::Bool(true)),
        preserve_entry_signatures: Some(PreserveEntrySignatures::Strict),
        shim_missing_exports: Some(true),
        ..Default::default()
    };
    let mut bundler = Bundler::with_plugins(
        options,
        vec![Arc::new(RuntimePathPlugin {
            dependencies: Arc::new(HashMap::new()),
        })],
    )
    .map_err(|errors| format!("failed to create dependency optimizer: {errors:?}"))?;
    rolldown_runtime()
        .block_on(bundler.write())
        .map_err(|errors| format!("failed to optimize dependencies: {errors:?}"))?;
    for index in 0..specifiers.len() {
        let internal = staging.join(format!(
            "internal-dependency-{index}.{}",
            module_format.extension()
        ));
        let output = staging.join(format!("dependency-{index}.{}", module_format.extension()));
        write_dependency_proxy(&internal, &output, module_format)?;
    }
    fs::write(staging.join(".complete"), [])
        .map_err(|error| format!("failed to finalize {}: {error}", staging.display()))
}

fn cleanup_staging(staging: &Path, error: String) -> String {
    if !staging.exists() {
        return error;
    }
    match fs::remove_dir_all(staging) {
        Ok(()) => error,
        Err(cleanup_error) => format!(
            "{error}; failed to remove dependency cache staging directory {}: {cleanup_error}",
            staging.display()
        ),
    }
}

fn dependency_outputs(cache_dir: &Path, count: usize, module_format: ModuleFormat) -> Vec<PathBuf> {
    (0..count)
        .map(|index| cache_dir.join(format!("dependency-{index}.{}", module_format.extension())))
        .collect()
}

fn write_dependency_proxy(
    internal: &Path,
    output: &Path,
    module_format: ModuleFormat,
) -> Result<(), String> {
    let relative = format!("./{}", internal.file_name().unwrap().to_string_lossy());
    let quoted = serde_json::to_string(&relative).unwrap();
    if module_format == ModuleFormat::Cjs {
        return fs::write(output, format!("module.exports = require({quoted});\n"))
            .map_err(|error| format!("failed to write {}: {error}", output.display()));
    }

    let code = fs::read_to_string(internal)
        .map_err(|error| format!("failed to read {}: {error}", internal.display()))?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &code, SourceType::mjs()).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(format!(
            "failed to parse optimized dependency {}",
            internal.display()
        ));
    }
    let mut collector = CjsExportCollector::default();
    collector.visit_program(&parsed.program);
    let mut names = collector.names;
    names.remove("default");
    let mut proxy = format!(
        "import dependency from {quoted};\nexport * from {quoted};\nexport default dependency;\n"
    );
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort();
    for (index, name) in names.into_iter().enumerate() {
        let quoted_name = serde_json::to_string(&name).unwrap();
        proxy.push_str(&format!(
            "const __bastest_export_{index} = dependency[{quoted_name}];\nexport {{ __bastest_export_{index} as {quoted_name} }};\n"
        ));
    }
    fs::write(output, proxy)
        .map_err(|error| format!("failed to write {}: {error}", output.display()))
}

#[derive(Default)]
struct CjsExportCollector {
    names: HashSet<String>,
}

impl<'a> Visit<'a> for CjsExportCollector {
    fn visit_static_member_expression(
        &mut self,
        member: &oxc_ast::ast::StaticMemberExpression<'a>,
    ) {
        if matches!(&member.object, Expression::Identifier(identifier) if identifier.name == "exports")
        {
            self.names.insert(member.property.name.to_string());
        }
        walk::walk_static_member_expression(self, member);
    }

    fn visit_computed_member_expression(
        &mut self,
        member: &oxc_ast::ast::ComputedMemberExpression<'a>,
    ) {
        if matches!(&member.object, Expression::Identifier(identifier) if identifier.name == "exports")
            && let Expression::StringLiteral(property) = &member.expression
        {
            self.names.insert(property.value.to_string());
        }
        walk::walk_computed_member_expression(self, member);
    }
}

fn rolldown_runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("failed to create rolldown runtime"))
}

fn hash_dependency_state(start: &Path, hasher: &mut DefaultHasher) -> Result<(), String> {
    const DEPENDENCY_FILES: &[&str] = &[
        "package.json",
        "deno.lock",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "bun.lock",
    ];
    for directory in start.ancestors() {
        for filename in DEPENDENCY_FILES {
            let path = directory.join(filename);
            if path.is_file() {
                path.hash(hasher);
                fs::read(&path)
                    .map_err(|error| {
                        format!(
                            "failed to read dependency state {} for module cache: {error}",
                            path.display()
                        )
                    })?
                    .hash(hasher);
            }
        }
    }
    Ok(())
}

fn transform_jsx(file: &Path, source: &str) -> Result<String, String> {
    let filename = file.to_string_lossy();
    let result = enhanced_transform(
        &filename,
        source,
        EnhancedTransformOptions {
            jsx: Some(Either::Right(JsxOptions {
                runtime: Some("classic".to_string()),
                pragma: Some("React.createElement".to_string()),
                pragma_frag: Some("React.Fragment".to_string()),
                ..Default::default()
            })),
            typescript: Some(TypeScriptOptions::default()),
            tsconfig: Some(TsconfigOption::Disabled),
            sourcemap: true,
            ..Default::default()
        },
        false,
    );

    if !result.errors.is_empty() {
        return Err(format!(
            "failed to transform {}: {:?}",
            file.display(),
            result.errors
        ));
    }
    Ok(result.code)
}

fn rewrite_import_meta_test(source: &str) -> String {
    const TARGET: &str = "import.meta.test";
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    let bytes = source.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' | b'`' => {
                index = skip_string(bytes, index);
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = skip_line_comment(bytes, index + 2);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = skip_block_comment(bytes, index + 2);
            }
            _ if bytes[index..].starts_with(TARGET.as_bytes()) => {
                output.push_str(&source[cursor..index]);
                output.push_str("true");
                index += TARGET.len();
                cursor = index;
            }
            _ => {
                index += 1;
            }
        }
    }

    output.push_str(&source[cursor..]);
    output
}

fn skip_string(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
            continue;
        }
        if bytes[index] == quote {
            return index + 1;
        }
        index += 1;
    }
    bytes.len()
}

fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn skip_block_comment(bytes: &[u8], mut index: usize) -> usize {
    while index + 1 < bytes.len() {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

fn rewrite_module_specifiers(
    file: &Path,
    source: &str,
    package_root: &Path,
) -> Result<String, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(file).unwrap_or_else(|_| SourceType::mjs());
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(format!(
            "failed to parse {} for module transform",
            file.display()
        ));
    }

    let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
    let mut collector = ModuleSpecifierCollector::new(semantic.scoping());
    collector.visit_program(&parsed.program);
    let parent = file.parent().unwrap_or_else(|| Path::new("."));
    let runtime = runtime_module_specifier(package_root)?;
    let mut replacements = Vec::new();
    for specifier in collector.specifiers {
        let replacement = if specifier.value == "bastest" {
            Some(runtime.clone())
        } else if is_relative_specifier(&specifier.value) {
            Some(absolute_module_specifier(&parent.join(&specifier.value))?)
        } else {
            None
        };
        if let Some(replacement) = replacement {
            replacements.push(SpanReplacement {
                start: specifier.span.start as usize,
                end: specifier.span.end as usize,
                text: serde_json::to_string(&replacement).unwrap(),
            });
        }
    }
    Ok(apply_span_replacements(source, &mut replacements))
}

struct ModuleSpecifierCollector<'s> {
    scoping: &'s Scoping,
    specifiers: Vec<ModuleSpecifier>,
}

struct ModuleSpecifier {
    span: Span,
    value: String,
}

impl<'s> ModuleSpecifierCollector<'s> {
    fn new(scoping: &'s Scoping) -> Self {
        Self {
            scoping,
            specifiers: Vec::new(),
        }
    }

    fn push(&mut self, literal: &StringLiteral<'_>) {
        self.specifiers.push(ModuleSpecifier {
            span: literal.span,
            value: literal.value.to_string(),
        });
    }
}

impl<'a> Visit<'a> for ModuleSpecifierCollector<'_> {
    fn visit_import_declaration(&mut self, declaration: &ImportDeclaration<'a>) {
        let type_only = declaration.import_kind == ImportOrExportKind::Type
            || declaration.specifiers.as_ref().is_some_and(|specifiers| {
                !specifiers.is_empty()
                    && specifiers.iter().all(|specifier| {
                        matches!(
                            specifier,
                            ImportDeclarationSpecifier::ImportSpecifier(specifier)
                                if specifier.import_kind == ImportOrExportKind::Type
                        )
                    })
            });
        if !type_only {
            self.push(&declaration.source);
        }
        walk::walk_import_declaration(self, declaration);
    }

    fn visit_export_named_declaration(&mut self, declaration: &ExportNamedDeclaration<'a>) {
        if declaration.export_kind != ImportOrExportKind::Type
            && let Some(source) = &declaration.source
        {
            self.push(source);
        }
        walk::walk_export_named_declaration(self, declaration);
    }

    fn visit_export_all_declaration(&mut self, declaration: &ExportAllDeclaration<'a>) {
        self.push(&declaration.source);
        walk::walk_export_all_declaration(self, declaration);
    }

    fn visit_import_expression(&mut self, expression: &ImportExpression<'a>) {
        if let Expression::StringLiteral(source) = &expression.source {
            self.push(source);
        }
        walk::walk_import_expression(self, expression);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if matches!(&call.callee, Expression::Identifier(identifier) if identifier.is_global_reference_name("require".into(), self.scoping))
            && let Some(Argument::StringLiteral(source)) = call.arguments.first()
        {
            self.push(source);
        }
        walk::walk_call_expression(self, call);
    }
}

struct SpanReplacement {
    start: usize,
    end: usize,
    text: String,
}

fn apply_span_replacements(source: &str, replacements: &mut [SpanReplacement]) -> String {
    replacements.sort_by_key(|replacement| replacement.start);
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for replacement in replacements {
        output.push_str(&source[cursor..replacement.start]);
        output.push_str(&replacement.text);
        cursor = replacement.end;
    }
    output.push_str(&source[cursor..]);
    output
}

fn is_relative_specifier(specifier: &str) -> bool {
    matches!(specifier, "." | "..") || specifier.starts_with("./") || specifier.starts_with("../")
}

fn absolute_module_specifier(path: &Path) -> Result<String, String> {
    std::path::absolute(path)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| format!("failed to resolve module path {}: {error}", path.display()))
}

fn absolute_external_specifier(path: &Path) -> Result<String, String> {
    let path = std::path::absolute(path)
        .map_err(|error| format!("failed to resolve module path {}: {error}", path.display()))?;
    let mut specifier = String::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(disk) | Prefix::VerbatimDisk(disk) => {
                    specifier.push(char::from(disk));
                    specifier.push(':');
                }
                Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                    specifier.push_str("//");
                    specifier.push_str(&server.to_string_lossy());
                    specifier.push('/');
                    specifier.push_str(&share.to_string_lossy());
                }
                Prefix::DeviceNS(device) => {
                    specifier.push_str("//./");
                    specifier.push_str(&device.to_string_lossy());
                }
                Prefix::Verbatim(value) => {
                    specifier.push_str("//?/");
                    specifier.push_str(&value.to_string_lossy());
                }
            },
            Component::RootDir => {
                if !specifier.ends_with('/') {
                    specifier.push('/');
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !specifier.ends_with('/') {
                    specifier.push('/');
                }
                specifier.push_str("..");
            }
            Component::Normal(value) => {
                if !specifier.ends_with('/') {
                    specifier.push('/');
                }
                specifier.push_str(&value.to_string_lossy());
            }
        }
    }
    Ok(specifier)
}

fn rewrite_assert_calls(file: &Path, source: &str) -> Result<String, String> {
    if !source.contains("assert(") && !source.contains("assertSnapshot(") {
        return Ok(source.to_string());
    }

    let allocator = Allocator::default();
    let source_type = SourceType::from_path(file).unwrap_or_else(|_| SourceType::mjs());
    let parsed = Parser::new(&allocator, source, source_type)
        .with_options(ParseOptions {
            parse_regular_expression: true,
            ..ParseOptions::default()
        })
        .parse();
    if !parsed.diagnostics.is_empty() {
        return Err(format!(
            "failed to parse {} for assert transform",
            file.display()
        ));
    }

    let mut collector = AssertTransformCollector {
        source,
        replacements: Vec::new(),
    };
    collector.visit_program(&parsed.program);
    Ok(apply_replacements(source, &mut collector.replacements))
}

struct Replacement {
    index: usize,
    text: String,
}

struct AssertTransformCollector<'a> {
    source: &'a str,
    replacements: Vec<Replacement>,
}

impl<'a> Visit<'a> for AssertTransformCollector<'_> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Some(first_argument) = call.arguments.first() {
            let expression_span = first_argument.span();
            let expression = source_text(self.source, expression_span).trim();
            if is_assert_callee(&call.callee) {
                let captures = collect_captures(self.source, first_argument);
                let metadata = power_assert_metadata(expression, captures);
                let last_argument = call.arguments.last().unwrap_or(first_argument);
                self.replacements.push(Replacement {
                    index: last_argument.span().end as usize,
                    text: format!(", {metadata}"),
                });
            } else if is_assert_snapshot_callee(&call.callee) {
                let last_argument = call.arguments.last().unwrap_or(first_argument);
                let metadata = snapshot_metadata(expression);
                self.replacements.push(Replacement {
                    index: last_argument.span().end as usize,
                    text: format!(", {metadata}"),
                });
            }
        }
        walk::walk_call_expression(self, call);
    }
}

fn is_assert_callee(callee: &Expression<'_>) -> bool {
    matches!(callee, Expression::Identifier(identifier) if identifier.name == "assert")
}

fn is_assert_snapshot_callee(callee: &Expression<'_>) -> bool {
    matches!(callee, Expression::Identifier(identifier) if identifier.name == "assertSnapshot")
}

fn power_assert_metadata(expression: &str, captures: Vec<Capture>) -> String {
    let captures = captures
        .into_iter()
        .map(|capture| {
            format!(
                "[{}, {}, {}, () => ({})]",
                serde_json::to_string(&capture.source).unwrap(),
                capture.start,
                capture.end,
                capture.source
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{{ expression: {}, captures: [{}] }}",
        serde_json::to_string(expression).unwrap(),
        captures
    )
}

fn snapshot_metadata(expression: &str) -> String {
    format!(
        "{{ expression: {} }}",
        serde_json::to_string(expression).unwrap()
    )
}

#[derive(Clone)]
struct Capture {
    source: String,
    start: u32,
    end: u32,
}

fn collect_captures(source: &str, argument: &Argument<'_>) -> Vec<Capture> {
    let mut collector = CaptureCollector {
        source,
        argument_start: argument.span().start,
        captures: Vec::new(),
    };
    collector.visit_argument(argument);
    collector
        .captures
        .sort_by_key(|capture| (capture.start, capture.end));
    collector
        .captures
        .dedup_by(|left, right| left.start == right.start && left.end == right.end);
    collector
        .captures
        .into_iter()
        .filter(|capture| {
            !capture
                .source
                .chars()
                .all(|character| character.is_ascii_digit())
        })
        .collect()
}

struct CaptureCollector<'a> {
    source: &'a str,
    argument_start: u32,
    captures: Vec<Capture>,
}

impl<'a> Visit<'a> for CaptureCollector<'_> {
    fn visit_expression(&mut self, expression: &Expression<'a>) {
        match expression {
            Expression::Identifier(identifier) => self.push(identifier.span),
            Expression::StaticMemberExpression(member) => self.push(member.span),
            _ => {}
        }
        walk::walk_expression(self, expression);
    }
}

impl CaptureCollector<'_> {
    fn push(&mut self, span: Span) {
        let text = source_text(self.source, span).trim();
        if !text.is_empty() {
            self.captures.push(Capture {
                source: text.to_string(),
                start: span.start - self.argument_start,
                end: span.end - self.argument_start,
            });
        }
    }
}

fn source_text(source: &str, span: Span) -> &str {
    &source[span.start as usize..span.end as usize]
}

fn apply_replacements(source: &str, replacements: &mut Vec<Replacement>) -> String {
    replacements.sort_by_key(|replacement| replacement.index);
    let mut output = String::with_capacity(
        source.len()
            + replacements
                .iter()
                .map(|replacement| replacement.text.len())
                .sum::<usize>(),
    );
    let mut cursor = 0;
    for replacement in replacements {
        output.push_str(&source[cursor..replacement.index]);
        output.push_str(&replacement.text);
        cursor = replacement.index;
    }
    output.push_str(&source[cursor..]);
    output
}

fn runtime_module_specifier(package_root: &Path) -> Result<String, String> {
    let dist = package_root.join("dist");
    let source = package_root.join("src");
    let file = if dist.is_dir() {
        dist.join("mod.mjs")
    } else {
        source.join("mod.ts")
    };
    absolute_module_specifier(&file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_globals_ignore_strings_comments_and_shadowed_bindings() {
        let source = r#"
            const text = "__dirname";
            // __filename
            const __filename = "local";
            console.log(text, __filename);
        "#;
        assert!(
            rewrite_bundled_module(Path::new("C:/virtual/module.js"), source, &HashMap::new())
                .is_none()
        );
    }

    #[test]
    fn runtime_globals_inject_unresolved_node_bindings() {
        let source = "console.log(__filename, __dirname);";
        let transformed =
            rewrite_bundled_module(Path::new("C:/virtual/module.js"), source, &HashMap::new())
                .unwrap();
        assert!(transformed.contains("var __filename ="));
        assert!(transformed.contains("var __dirname ="));
    }

    #[test]
    fn runtime_globals_preserve_directives_and_hashbangs() {
        let directive = "\"use strict\";\nconsole.log(__filename);";
        let transformed = rewrite_bundled_module(
            Path::new("C:/virtual/module.js"),
            directive,
            &HashMap::new(),
        )
        .unwrap();
        assert!(transformed.starts_with("\"use strict\";\nvar __filename ="));

        let hashbang = "#!/usr/bin/env node\nconsole.log(__filename);";
        let transformed =
            rewrite_bundled_module(Path::new("C:/virtual/module.js"), hashbang, &HashMap::new())
                .unwrap();
        assert!(transformed.starts_with("#!/usr/bin/env node\nvar __filename ="));
    }

    #[test]
    fn runtime_paths_ignore_shadowed_require_resolve() {
        let source =
            "const require = { resolve: value => value }; require.resolve('./fixture.js');";
        assert!(
            rewrite_bundled_module(Path::new("C:/virtual/module.js"), source, &HashMap::new(),)
                .is_none()
        );
    }

    #[test]
    fn dependency_detection_only_accepts_package_specifiers() {
        for specifier in [
            "jsdom",
            "jsdom/lib/api.js",
            "@scope/package",
            "_legacy",
            "sqlite",
            "test",
        ] {
            assert!(
                is_bare_dependency(specifier),
                "expected {specifier} to be bare"
            );
        }
        for specifier in [
            "",
            "bastest",
            "fs",
            "fs/promises",
            "node:path",
            "node:sqlite",
            "node:test",
            ".",
            "..",
            "./local.js",
            "../local.js",
            "/absolute.js",
            "C:/absolute.js",
            r"\\server\share\module.js",
            "file:///module.js",
            "data:text/javascript,export default 1",
            "https://example.com/module.js",
            "npm:jsdom",
            "#internal",
            "@scope",
            ".hidden-package",
            "package//subpath",
            "package/../subpath",
        ] {
            assert!(
                !is_bare_dependency(specifier),
                "expected {specifier} not to be bare"
            );
        }
    }

    #[test]
    fn module_collection_excludes_type_only_imports() {
        let allocator = Allocator::default();
        let parsed = Parser::new(
            &allocator,
            "import type { Foo } from 'types-only'; import { value } from 'runtime';",
            SourceType::ts(),
        )
        .parse();
        let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
        let mut collector = ModuleSpecifierCollector::new(semantic.scoping());
        collector.visit_program(&parsed.program);
        let modules = collector
            .specifiers
            .into_iter()
            .map(|specifier| specifier.value)
            .collect::<Vec<_>>();
        assert_eq!(modules, ["runtime"]);
    }

    #[test]
    fn module_collection_only_includes_global_require() {
        let allocator = Allocator::default();
        let parsed = Parser::new(
            &allocator,
            "const value = require('global-dependency'); function local(require) { return require('local-dependency'); }",
            SourceType::mjs(),
        )
        .parse();
        let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
        let mut collector = ModuleSpecifierCollector::new(semantic.scoping());
        collector.visit_program(&parsed.program);
        let modules = collector
            .specifiers
            .into_iter()
            .map(|specifier| specifier.value)
            .collect::<Vec<_>>();
        assert_eq!(modules, ["global-dependency"]);
    }

    #[test]
    fn cjs_exports_are_collected_from_ast_only() {
        let allocator = Allocator::default();
        let parsed = Parser::new(
            &allocator,
            r#"exports.named = 1; exports["computed"] = 2; const text = "exports.fake";"#,
            SourceType::mjs(),
        )
        .parse();
        let mut collector = CjsExportCollector::default();
        collector.visit_program(&parsed.program);
        assert!(collector.names.contains("named"));
        assert!(collector.names.contains("computed"));
        assert!(!collector.names.contains("fake"));
    }
}
