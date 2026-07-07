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
    let rewritten = rewrite_assert_calls(
        file,
        &rewrite_relative_imports(
            file,
            &rewrite_runtime_imports(&source, package_root, &temp_dir)?,
        )?,
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
    Ok(rewrite_bastest_imports(
        &rewrite_import_meta_test(source),
        &bastest_module,
    ))
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

fn rewrite_bastest_imports(source: &str, module: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    let mut chars = source.char_indices().peekable();

    while let Some((start, character)) = chars.next() {
        if character != '"' && character != '\'' {
            continue;
        }

        let quote = character;
        let mut end = None;
        let mut escaped = false;
        for (index, current) in chars.by_ref() {
            if escaped {
                escaped = false;
                continue;
            }
            if current == '\\' {
                escaped = true;
                continue;
            }
            if current == quote {
                end = Some(index);
                break;
            }
        }

        let Some(end) = end else {
            break;
        };
        let specifier = &source[start + quote.len_utf8()..end];
        if specifier != "bastest" || !is_static_import_specifier(&source[..start]) {
            continue;
        }

        output.push_str(&source[cursor..start + quote.len_utf8()]);
        output.push_str(module);
        cursor = end;
    }

    output.push_str(&source[cursor..]);
    output
}

fn rewrite_relative_imports(file: &Path, source: &str) -> Result<String, String> {
    let Some(parent) = file.parent() else {
        return Ok(source.to_string());
    };

    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    let mut chars = source.char_indices().peekable();

    while let Some((start, character)) = chars.next() {
        if character != '"' && character != '\'' {
            continue;
        }

        let quote = character;
        let mut end = None;
        let mut escaped = false;
        for (index, current) in chars.by_ref() {
            if escaped {
                escaped = false;
                continue;
            }
            if current == '\\' {
                escaped = true;
                continue;
            }
            if current == quote {
                end = Some(index);
                break;
            }
        }

        let Some(end) = end else {
            break;
        };
        let specifier = &source[start + quote.len_utf8()..end];
        if !is_relative_specifier(specifier) || !is_static_import_specifier(&source[..start]) {
            continue;
        }

        let resolved = parent.join(specifier);
        let url = url::Url::from_file_path(&resolved).map_err(|_| {
            format!(
                "failed to create file URL for import {} in {}",
                specifier,
                file.display()
            )
        })?;
        output.push_str(&source[cursor..start + quote.len_utf8()]);
        output.push_str(url.as_str());
        cursor = end;
    }

    output.push_str(&source[cursor..]);
    Ok(output)
}

fn is_relative_specifier(specifier: &str) -> bool {
    specifier.starts_with("./") || specifier.starts_with("../")
}

fn is_static_import_specifier(before: &str) -> bool {
    let before = before.trim_end();
    before.ends_with("from") || before.ends_with("import")
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

fn runtime_module_specifier(package_root: &Path, _output_dir: &Path) -> Result<String, String> {
    let dist = package_root.join("dist");
    let source = package_root.join("src");
    let file = if dist.is_dir() {
        dist.join("index.mjs")
    } else {
        source.join("mod.ts")
    };
    let specifier = url::Url::from_file_path(&file).map_err(|_| {
        format!(
            "failed to create file URL for runtime module {}",
            file.display()
        )
    })?;
    Ok(specifier.to_string())
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
