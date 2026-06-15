use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use oxc_allocator::Allocator;
use oxc_ast::ast::{Argument, CallExpression, Expression};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::{ParseOptions, Parser};
use oxc_span::{GetSpan, SourceType, Span};
use rolldown::{
    Either, EnhancedTransformOptions, JsxOptions, TsconfigOption, TypeScriptOptions,
    enhanced_transform,
};

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn transform_test_file(file: &Path, package_root: &Path) -> Result<PathBuf, String> {
    let temp_dir = make_temp_dir()?;
    let extension = file
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let output_extension = if matches!(extension, "jsx" | "tsx") || extension.is_empty() {
        "js"
    } else {
        extension
    };
    let output = temp_dir.join(format!("test.{output_extension}"));

    let source = fs::read_to_string(file)
        .map_err(|error| format!("failed to read {}: {error}", file.display()))?;
    let rewritten = rewrite_power_assert(
        file,
        &rewrite_runtime_imports(&source, package_root, &temp_dir)?,
    )?;
    let code = if matches!(extension, "jsx" | "tsx") {
        transform_jsx(file, &rewritten)?
    } else {
        rewritten
    };

    fs::write(&output, code).map_err(|error| {
        format!(
            "failed to write transformed test {}: {error}",
            output.display()
        )
    })?;
    Ok(output)
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

fn rewrite_runtime_imports(
    source: &str,
    package_root: &Path,
    output_dir: &Path,
) -> Result<String, String> {
    let bastest_module = runtime_module_specifier(package_root, output_dir)?;
    Ok(source
        .replace("import.meta.test", "true")
        .replace(
            "\"bastest\"",
            &serde_json::to_string(&bastest_module).unwrap(),
        )
        .replace(
            "'bastest'",
            &serde_json::to_string(&bastest_module).unwrap(),
        ))
}

fn rewrite_power_assert(file: &Path, source: &str) -> Result<String, String> {
    if !source.contains("assert(") {
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
    if !parsed.errors.is_empty() {
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
        if is_assert_callee(&call.callee)
            && let Some(first_argument) = call.arguments.first()
        {
            let expression_span = first_argument.span();
            let expression = source_text(self.source, expression_span).trim();
            let captures = collect_captures(self.source, first_argument);
            let metadata = power_assert_metadata(expression, captures);
            let last_argument = call.arguments.last().unwrap_or(first_argument);
            self.replacements.push(Replacement {
                index: last_argument.span().end as usize,
                text: format!(", {metadata}"),
            });
        }
        walk::walk_call_expression(self, call);
    }
}

fn is_assert_callee(callee: &Expression<'_>) -> bool {
    matches!(callee, Expression::Identifier(identifier) if identifier.name == "assert")
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

fn runtime_module_specifier(package_root: &Path, output_dir: &Path) -> Result<String, String> {
    let dist = package_root.join("dist");
    let source = package_root.join("src");
    let file = if dist.is_dir() {
        dist.join("index.mjs")
    } else {
        source.join("mod.ts")
    };
    let relative = relative_path(output_dir, &file).ok_or_else(|| {
        format!(
            "failed to create relative import from {} to {}",
            output_dir.display(),
            file.display()
        )
    })?;
    let mut specifier = relative.to_string_lossy().replace('\\', "/");
    if !specifier.starts_with('.') {
        specifier = format!("./{specifier}");
    }
    Ok(specifier)
}

fn relative_path(from_dir: &Path, to_file: &Path) -> Option<PathBuf> {
    let from = from_dir.components().collect::<Vec<_>>();
    let to = to_file.components().collect::<Vec<_>>();
    if from.first() != to.first() {
        return None;
    }

    let mut shared = 0;
    while shared < from.len() && shared < to.len() && from[shared] == to[shared] {
        shared += 1;
    }

    let mut output = PathBuf::new();
    for _ in shared..from.len() {
        output.push("..");
    }
    for component in &to[shared..] {
        output.push(component.as_os_str());
    }
    Some(output)
}

fn make_temp_dir() -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("bastest-{}-{nonce}-{counter}", std::process::id()));
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create temp dir {}: {error}", dir.display()))?;
    Ok(dir)
}
