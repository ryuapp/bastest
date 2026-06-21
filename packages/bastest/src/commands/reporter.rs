use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

const MAX_CAPTURE_VALUE_LINES: usize = 6;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileResult {
    pub file: String,
    pub duration_ms: f64,
    pub tests: Vec<TestResult>,
    pub load_error: Option<SerializedError>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    pub duration_ms: f64,
    pub error: Option<SerializedError>,
    pub steps: Vec<StepResult>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub name: String,
    pub status: TestStatus,
    pub duration_ms: f64,
    pub error: Option<SerializedError>,
    pub steps: Vec<StepResult>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed,
    Ignored,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SerializedError {
    pub name: String,
    pub message: String,
    pub stack: Option<String>,
    pub actual: Option<Value>,
    pub expected: Option<Value>,
    pub operator: Option<String>,
    pub expression: Option<String>,
    pub captures: Option<Vec<AssertionCapture>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AssertionCapture {
    pub source: String,
    pub start: usize,
    pub end: usize,
    pub value: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub passed: usize,
    pub failed: usize,
    pub ignored: usize,
    pub load_errors: usize,
    pub duration_ms: f64,
}

pub fn report_stream_test(
    file: &Path,
    cwd: &Path,
    test: &TestResult,
    agent: bool,
    file_printed: &mut bool,
) {
    if agent && test.status != TestStatus::Failed && !has_failed_step(&test.steps) {
        return;
    }

    if !*file_printed {
        let relative = file.strip_prefix(cwd).unwrap_or(file);
        let path = display_path(relative);
        println!(
            "\n{}",
            if test.status == TestStatus::Failed || has_failed_step(&test.steps) {
                red(&path)
            } else {
                dim(&path)
            }
        );
        *file_printed = true;
    }

    print_test(test, 2, agent);
}

pub fn report_file(file: &FileResult, cwd: &Path, agent: bool) {
    if agent && !file_failed(file) {
        return;
    }

    let relative = Path::new(&file.file)
        .strip_prefix(cwd)
        .unwrap_or_else(|_| Path::new(&file.file));
    let file_failed = file_failed(file);
    println!(
        "\n{} {}",
        if file_failed {
            red(&display_path(relative))
        } else {
            dim(&display_path(relative))
        },
        dim(&format!("({})", format_ms(file.duration_ms)))
    );

    if let Some(error) = &file.load_error {
        println!(
            "  {} failed to load {}",
            status_label(&TestStatus::Failed),
            error.message
        );
        if let Some(stack) = &error.stack {
            println!("{}", dim(&indent(&clean_stack(stack), 4)));
        }
        return;
    }

    for test in &file.tests {
        print_test(test, 2, agent);
    }
}

pub fn report_summary(results: &[FileResult]) -> Summary {
    let summary = summarize(results);
    print_summary(&summary, false);
    summary
}

pub fn report_summary_compact(results: &[FileResult]) -> Summary {
    let summary = summarize(results);
    print_summary(&summary, true);
    summary
}

fn summarize(results: &[FileResult]) -> Summary {
    let mut summary = Summary {
        passed: 0,
        failed: 0,
        ignored: 0,
        load_errors: 0,
        duration_ms: 0.0,
    };

    for file in results {
        if file.load_error.is_some() {
            summary.load_errors += 1;
            continue;
        }
        for test in &file.tests {
            count(&mut summary, &test.status);
        }
    }

    summary.duration_ms = round_2(results.iter().map(|file| file.duration_ms).sum());
    summary
}

fn print_test(test: &TestResult, spaces: usize, agent: bool) {
    if agent && test.status != TestStatus::Failed && !has_failed_step(&test.steps) {
        return;
    }

    println!(
        "{}{} ... {} {}",
        " ".repeat(spaces),
        status_text(&test.status, &test.name),
        status_label(&test.status),
        dim(&format!("({})", format_ms(test.duration_ms))),
    );
    for step in &test.steps {
        print_step(step, spaces + 2, agent);
    }
    if let Some(error) = &test.error
        && !has_failed_step(&test.steps)
    {
        print_error(error, spaces + 2);
    }
}

fn print_step(step: &StepResult, spaces: usize, agent: bool) {
    if agent && step.status != TestStatus::Failed && !has_failed_step(&step.steps) {
        return;
    }

    println!(
        "{}{} ... {} {}",
        " ".repeat(spaces),
        status_text(&step.status, &step.name),
        status_label(&step.status),
        dim(&format!("({})", format_ms(step.duration_ms))),
    );
    for child in &step.steps {
        print_step(child, spaces + 2, agent);
    }
    if let Some(error) = &step.error
        && !has_failed_step(&step.steps)
    {
        print_error(error, spaces + 2);
    }
}

fn count(summary: &mut Summary, status: &TestStatus) {
    match status {
        TestStatus::Passed => summary.passed += 1,
        TestStatus::Failed => summary.failed += 1,
        TestStatus::Ignored => summary.ignored += 1,
    }
}

fn file_failed(file: &FileResult) -> bool {
    file.load_error.is_some()
        || file
            .tests
            .iter()
            .any(|test| test.status == TestStatus::Failed)
}

fn print_summary(summary: &Summary, compact: bool) {
    let failed = summary.failed + summary.load_errors;
    let status = if failed > 0 {
        red_bold("fail")
    } else {
        green_bold("ok")
    };
    println!(
        "\n{} | {} passed | {} failed | {} ignored {}",
        status,
        summary.passed,
        failed,
        summary.ignored,
        dim(&format!("({})", format_seconds(summary.duration_ms)))
    );
    if !compact {
        println!();
    }
}

fn print_error(error: &SerializedError, spaces: usize) {
    let padding = " ".repeat(spaces);
    println!("{}{}: {}", padding, red_bold(&error.name), error.message);

    if let Some(expression) = &error.expression {
        println!();
        print_power_assert(expression, error.captures.as_deref(), spaces);
    }

    if error.expression.is_none() && (error.actual.is_some() || error.expected.is_some()) {
        println!();
        print_value_block(
            "actual",
            error.actual.as_ref(),
            spaces + 2,
            error.operator.as_deref(),
        );
        print_value_block(
            "expected",
            error.expected.as_ref(),
            spaces + 2,
            error.operator.as_deref(),
        );
    }

    if let Some(frame) = first_user_frame(error.stack.as_deref()) {
        println!();
        println!("{}{}", padding, dim(frame.trim()));
    } else if let Some(stack) = &error.stack {
        println!("{}", dim(&indent(&clean_stack(stack), spaces)));
    }
}

fn print_power_assert(expression: &str, captures: Option<&[AssertionCapture]>, spaces: usize) {
    let padding = " ".repeat(spaces);
    let assert_source = format!("assert({expression})");
    println!("{}{}", padding, assert_source);

    let Some(captures) = captures else {
        return;
    };
    if captures.is_empty() {
        return;
    }

    let offset = "assert(".len();
    let mut sorted = captures.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|capture| (capture.start, std::cmp::Reverse(capture.end)));

    let mut marker = vec![' '; assert_source.chars().count()];
    for capture in &sorted {
        let _ = &capture.source;
        let index = offset + capture.start;
        if index < marker.len() {
            marker[index] = '|';
        }
    }
    println!("{}{}", padding, marker.into_iter().collect::<String>());

    for capture in sorted.into_iter().rev() {
        let column = offset + capture.start;
        let value = format_json_value(&capture.value, Some("assert"));
        let available = terminal_width().saturating_sub(spaces + column).max(16);
        for line in value.lines().flat_map(|line| wrap_line(line, available)) {
            println!("{}{}{}", padding, " ".repeat(column), line);
        }
    }
}

fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if line.chars().count() <= width {
        return vec![line.to_string()];
    }
    if is_json_key_value_string_line(line) {
        return wrap_json_key_value_string_line(line, width);
    }
    if is_json_string_line(line) {
        return wrap_json_string_line(line, width);
    }

    let continuation = continuation_prefix(line);
    let continuation_width = continuation.chars().count();
    let continuation_content_width = width.saturating_sub(continuation_width).max(16);
    let mut wrapped = Vec::new();
    let mut current = String::new();
    let mut current_width = width;
    for character in line.chars() {
        current.push(character);
        if current.chars().count() >= current_width {
            wrapped.push(current);
            current = continuation.clone();
            current_width = continuation_width + continuation_content_width;
        }
    }
    if current.chars().count() > continuation_width {
        wrapped.push(current);
    }
    truncate_wrapped_lines(&mut wrapped, continuation, false);
    wrapped
}

fn wrap_json_key_value_string_line(line: &str, width: usize) -> Vec<String> {
    let value_start = line.find(": \"").unwrap_or(0) + ": \"".len();
    wrap_quoted_content(line, value_start, width)
}

fn wrap_json_string_line(line: &str, width: usize) -> Vec<String> {
    let value_start = line.find('"').unwrap_or(0) + 1;
    wrap_quoted_content(line, value_start, width)
}

fn wrap_quoted_content(line: &str, value_start: usize, width: usize) -> Vec<String> {
    let suffix = 1;
    let content = &line[value_start..line.len() - suffix];
    let continuation = " ".repeat(value_start);
    let first_width = width.saturating_sub(value_start + suffix).max(16);
    let continuation_width = width
        .saturating_sub(continuation.chars().count() + suffix)
        .max(16);

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_width = first_width;
    for character in content.chars() {
        current.push(character);
        if current.chars().count() >= current_width {
            chunks.push(current);
            current = String::new();
            current_width = continuation_width;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.len() > MAX_CAPTURE_VALUE_LINES {
        chunks.truncate(MAX_CAPTURE_VALUE_LINES - 1);
        chunks.push("...".to_string());
    }

    let last_index = chunks.len().saturating_sub(1);
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            if index == 0 && index == last_index {
                format!("{}{}\"", &line[..value_start], chunk)
            } else if index == 0 {
                format!("{}{}", &line[..value_start], chunk)
            } else if index == last_index {
                format!("{continuation}{chunk}\"")
            } else {
                format!("{continuation}{chunk}")
            }
        })
        .collect()
}

fn truncate_wrapped_lines(lines: &mut Vec<String>, continuation: String, quoted: bool) {
    if lines.len() <= MAX_CAPTURE_VALUE_LINES {
        return;
    }
    lines.truncate(MAX_CAPTURE_VALUE_LINES - 1);
    lines.push(format!(
        "{}...{}",
        continuation,
        if quoted { "\"" } else { "" }
    ));
}

fn is_json_key_value_string_line(line: &str) -> bool {
    line.contains(": \"") && line.trim_end().ends_with('"')
}

fn is_json_string_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 2
}

fn continuation_prefix(line: &str) -> String {
    let Some(value_start) = line.find(": \"").map(|index| index + ": \"".len()) else {
        return String::new();
    };
    " ".repeat(value_start)
}

fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100)
}

fn print_value_block(label: &str, value: Option<&Value>, spaces: usize, operator: Option<&str>) {
    let padding = " ".repeat(spaces);
    let formatted = value
        .map(|value| format_json_value(value, operator))
        .unwrap_or_else(|| "undefined".to_string());
    println!("{}{}", padding, value_label(label));
    for line in formatted.lines() {
        println!("{}  {}", padding, line);
    }
}

fn first_user_frame(stack: Option<&str>) -> Option<String> {
    let stack = stack?;
    clean_stack(stack)
        .lines()
        .skip(1)
        .find(|line| {
            let normalized = line.replace('\\', "/");
            !normalized.contains("/src/runtime.ts")
                && !normalized.contains("/src/asserts/")
                && !normalized.contains("node:internal/")
        })
        .map(ToOwned::to_owned)
}

fn has_failed_step(steps: &[StepResult]) -> bool {
    steps
        .iter()
        .any(|step| step.status == TestStatus::Failed || has_failed_step(&step.steps))
}

fn clean_stack(stack: &str) -> String {
    stack
        .lines()
        .filter(|line| !line.contains("/src/runtime.ts") && !line.contains("\\src\\runtime.ts"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn indent(value: &str, spaces: usize) -> String {
    let padding = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{padding}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn status_label(status: &TestStatus) -> String {
    match status {
        TestStatus::Passed => green_bold("ok"),
        TestStatus::Failed => red_bold("fail"),
        TestStatus::Ignored => yellow_bold("skip"),
    }
}

fn status_text(status: &TestStatus, value: &str) -> String {
    match status {
        TestStatus::Passed => value.to_string(),
        TestStatus::Failed => red(value),
        TestStatus::Ignored => dim(value),
    }
}

fn format_ms(ms: f64) -> String {
    format!("{}ms", ms.round())
}

fn format_json_value(value: &Value, operator: Option<&str>) -> String {
    if operator == Some("assert") && value.is_object() {
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    } else {
        value.to_string()
    }
}

fn format_seconds(ms: f64) -> String {
    if ms < 10_000.0 {
        format_ms(ms)
    } else {
        format!("{:.2}s", ms / 1000.0)
    }
}

fn display_path(path: &Path) -> String {
    let value = path.display().to_string();
    if cfg!(windows) {
        value.replace('\\', "/")
    } else {
        value
    }
}

fn round_2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn dim(value: &str) -> String {
    if color_enabled() {
        value.dimmed().to_string()
    } else {
        value.to_string()
    }
}

fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

fn red_bold(value: &str) -> String {
    if color_enabled() {
        value.red().bold().to_string()
    } else {
        value.to_string()
    }
}

fn red(value: &str) -> String {
    if color_enabled() {
        value.red().to_string()
    } else {
        value.to_string()
    }
}

fn green_bold(value: &str) -> String {
    if color_enabled() {
        value.truecolor(22, 163, 74).bold().to_string()
    } else {
        value.to_string()
    }
}

fn green(value: &str) -> String {
    if color_enabled() {
        value.truecolor(22, 163, 74).to_string()
    } else {
        value.to_string()
    }
}

fn yellow_bold(value: &str) -> String {
    if color_enabled() {
        value.yellow().bold().to_string()
    } else {
        value.to_string()
    }
}

fn value_label(label: &str) -> String {
    match label {
        "actual" => red(label),
        "expected" => green(label),
        _ => dim(label),
    }
}
