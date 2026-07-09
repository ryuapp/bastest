use std::path::{Path, PathBuf};

use owo_colors::OwoColorize;

pub fn run(cwd: &Path, package_root: &Path, files: &[PathBuf]) -> i32 {
    let Some(bin) = resolve_typescript_bin(cwd, package_root) else {
        eprintln!("failed to find local TypeScript compiler. Install typescript.");
        return 1;
    };
    let config_file = cwd.join("tsconfig.json");
    if !config_file.is_file() {
        eprintln!("failed to run typecheck: tsconfig.json was not found");
        return 1;
    }

    let assertions = match collect_type_assertions(files) {
        Ok(assertions) => assertions,
        Err(error) => {
            eprintln!("failed to read assert calls: {error}");
            return 1;
        }
    };

    if assertions.is_empty() {
        return 0;
    }

    match corsa::runtime::block_on(run_corsa_assert_type(&bin, cwd, &config_file, assertions)) {
        Ok(failures) if failures.is_empty() => 0,
        Ok(failures) => {
            eprintln!();
            eprintln!("Failed Type Assertions {}", failures.len());
            eprintln!();
            for failure in &failures {
                print_type_assertion_failure(cwd, failure);
            }
            eprintln!("{} type assertion errors", failures.len());
            1
        }
        Err(error) => {
            eprintln!("failed to run typecheck: {}", error.diagnostic());
            1
        }
    }
}

async fn run_corsa_assert_type(
    bin: &Path,
    cwd: &Path,
    config_file: &Path,
    assertions: Vec<TypeAssertion>,
) -> corsa::Result<Vec<TypeAssertionFailure>> {
    let open_file = assertions
        .first()
        .map(|assertion| display_path(Path::new(""), &assertion.file));
    let session = corsa::api::ProjectSession::spawn(
        corsa::api::ApiSpawnConfig::new(bin).with_cwd(cwd),
        display_path(Path::new(""), config_file),
        open_file.map(Into::into),
    )
    .await?;

    let mut failures = Vec::new();
    for assertion in assertions {
        let Some(actual_type) = session
            .get_type_at_position(
                assertion.file.display().to_string(),
                assertion.expression_position,
            )
            .await?
        else {
            failures.push(TypeAssertionFailure {
                file: assertion.file,
                line: assertion.expression_line,
                column: assertion.expression_column,
                expected: assertion.expected,
                actual: "unknown".to_string(),
                source_line: assertion.source_line,
            });
            continue;
        };

        let actual = if let Some(text) = actual_type.texts.first() {
            text.clone()
        } else {
            session.type_to_string(actual_type.id, None, None).await?
        };

        if normalize_type_text(&actual) != normalize_type_text(&assertion.expected) {
            failures.push(TypeAssertionFailure {
                file: assertion.file,
                line: assertion.expression_line,
                column: assertion.expression_column,
                expected: assertion.expected,
                actual,
                source_line: assertion.source_line,
            });
        }
    }
    session.close().await?;
    Ok(failures)
}

#[derive(Debug)]
struct TypeAssertion {
    file: PathBuf,
    expected: String,
    expression_position: u32,
    expression_line: usize,
    expression_column: usize,
    source_line: String,
}

#[derive(Debug)]
struct TypeAssertionFailure {
    file: PathBuf,
    line: usize,
    column: usize,
    expected: String,
    actual: String,
    source_line: String,
}

fn collect_type_assertions(files: &[PathBuf]) -> std::io::Result<Vec<TypeAssertion>> {
    let mut assertions = Vec::new();
    for file in files {
        let source = std::fs::read_to_string(file)?;
        assertions.extend(parse_type_assertions(file, &source));
    }
    Ok(assertions)
}

fn parse_type_assertions(file: &Path, source: &str) -> Vec<TypeAssertion> {
    let mut assertions = Vec::new();
    let mut offset = 0;

    while let Some(relative) = source[offset..].find("assert<") {
        let start = offset + relative;
        let type_start = start + "assert<".len();
        let Some(type_end) = find_matching_angle(source, type_start) else {
            offset = type_start;
            continue;
        };
        let call_start = skip_whitespace(source, type_end + 1);
        if !source[call_start..].starts_with('(') {
            offset = type_end + 1;
            continue;
        }

        let expression_start = skip_whitespace(source, call_start + 1);
        let Some(position_index) = first_expression_probe_position(source, expression_start) else {
            offset = call_start + 1;
            continue;
        };
        let (expression_line, expression_column) = line_column(source, position_index);
        assertions.push(TypeAssertion {
            file: file.to_path_buf(),
            expected: source[type_start..type_end].trim().to_string(),
            expression_position: utf16_position(source, position_index),
            expression_line,
            expression_column,
            source_line: source_line_at(source, position_index).to_string(),
        });
        offset = call_start + 1;
    }

    assertions
}

fn find_matching_angle(source: &str, start: usize) -> Option<usize> {
    let mut depth = 1;
    let mut index = start;
    let bytes = source.as_bytes();
    while index < bytes.len() {
        match bytes[index] {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            b'\'' | b'"' | b'`' => {
                index = skip_string(source, index)?;
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn skip_string(source: &str, start: usize) -> Option<usize> {
    let quote = source.as_bytes()[start];
    let mut index = start + 1;
    while index < source.len() {
        let byte = source.as_bytes()[index];
        if byte == b'\\' {
            index += 2;
            continue;
        }
        if byte == quote {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn first_expression_probe_position(source: &str, start: usize) -> Option<usize> {
    let start = skip_whitespace(source, start);
    source[start..]
        .char_indices()
        .find(|(_, char)| is_identifier_start(*char))
        .map(|(relative, _)| start + relative)
}

fn skip_whitespace(source: &str, start: usize) -> usize {
    let mut index = start;
    while let Some(char) = source[index..].chars().next() {
        if !char.is_whitespace() {
            break;
        }
        index += char.len_utf8();
    }
    index
}

fn is_identifier_start(char: char) -> bool {
    char == '_' || char == '$' || char.is_ascii_alphabetic()
}

fn utf16_position(source: &str, byte_index: usize) -> u32 {
    source[..byte_index].encode_utf16().count() as u32
}

fn line_column(source: &str, byte_index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for char in source[..byte_index].chars() {
        if char == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn source_line_at(source: &str, byte_index: usize) -> &str {
    let start = source[..byte_index]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let end = source[byte_index..]
        .find('\n')
        .map(|index| byte_index + index)
        .unwrap_or(source.len());
    source[start..end].trim_end_matches('\r')
}

fn normalize_type_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn print_type_assertion_failure(cwd: &Path, failure: &TypeAssertionFailure) {
    let file = display_path(cwd, &failure.file);
    let width = failure.line.to_string().len().max(2);
    let gutter = " ".repeat(width);
    let caret_padding = " ".repeat(failure.column.saturating_sub(1));

    eprintln!("{}  {} [ {} ]", red_bold("FAIL"), dim(&file), dim(&file));
    eprintln!(
        "{} Argument of type '{}' is not assignable to parameter of type '{}'.",
        red_bold("TypeCheckError:"),
        yellow(&failure.actual),
        green(&failure.expected)
    );
    eprintln!("  {} {}", green("Expected:"), failure.expected);
    eprintln!("  {}   {}", red("Actual:"), failure.actual);
    eprintln!(
        " {} {}:{}:{}",
        cyan(">"),
        dim(&file),
        failure.line,
        failure.column
    );
    eprintln!("{} {}", dim(&gutter), dim("|"));
    eprintln!(
        "{} {} {}",
        dim(&format!(
            "{line:>width$}",
            line = failure.line,
            width = width
        )),
        dim("|"),
        failure.source_line
    );
    eprintln!(
        "{} {} {}{}",
        dim(&gutter),
        dim("|"),
        caret_padding,
        red_bold("^")
    );
    eprintln!();
}

fn display_path(cwd: &Path, file: &Path) -> String {
    file.strip_prefix(cwd)
        .unwrap_or(file)
        .display()
        .to_string()
        .replace('\\', "/")
}

fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

fn red(value: &str) -> String {
    if color_enabled() {
        value.red().to_string()
    } else {
        value.to_string()
    }
}

fn red_bold(value: &str) -> String {
    if color_enabled() {
        value.red().bold().to_string()
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

fn yellow(value: &str) -> String {
    if color_enabled() {
        value.yellow().to_string()
    } else {
        value.to_string()
    }
}

fn cyan(value: &str) -> String {
    if color_enabled() {
        value.cyan().bold().to_string()
    } else {
        value.to_string()
    }
}

fn dim(value: &str) -> String {
    if color_enabled() {
        value.dimmed().to_string()
    } else {
        value.to_string()
    }
}

fn resolve_bin(cwd: &Path, package_root: &Path, name: &str) -> Option<PathBuf> {
    for start in [cwd, package_root] {
        let mut current = start.to_path_buf();
        loop {
            if let Some(bin) = bin_in(&current, name) {
                return Some(bin);
            }
            if !current.pop() {
                break;
            }
        }
    }
    None
}

fn resolve_typescript_bin(cwd: &Path, package_root: &Path) -> Option<PathBuf> {
    resolve_typescript_native_bin(cwd, package_root)
        .or_else(|| resolve_bin(cwd, package_root, "tsc"))
}

fn resolve_typescript_native_bin(cwd: &Path, package_root: &Path) -> Option<PathBuf> {
    let starts = resolve_package_dirs(cwd, package_root, "typescript");
    let platform_package = starts
        .iter()
        .find_map(|typescript_dir| typescript_native_package_name(typescript_dir))?;
    for start in starts {
        for ancestor in start.ancestors() {
            let package_dir = ancestor
                .join("node_modules")
                .join("@typescript")
                .join(&platform_package);
            let lib_dir = package_dir.join("lib");
            if let Some(bin) = native_tsc_in(&lib_dir) {
                return Some(bin);
            }
        }
    }
    None
}

fn typescript_native_package_name(typescript_dir: &Path) -> Option<String> {
    let package_json = std::fs::read_to_string(typescript_dir.join("package.json")).ok()?;
    let package: serde_json::Value = serde_json::from_str(&package_json).ok()?;
    let optional_dependencies = package.get("optionalDependencies")?.as_object()?;
    let platform_package = format!(
        "@typescript/typescript-{}-{}",
        typescript_platform()?,
        typescript_arch()?
    );
    if optional_dependencies.contains_key(&platform_package) {
        platform_package
            .strip_prefix("@typescript/")
            .map(str::to_string)
    } else {
        None
    }
}

fn resolve_package_dirs(cwd: &Path, package_root: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for start in [cwd, package_root] {
        let mut current = start.to_path_buf();
        loop {
            let candidate = current.join("node_modules").join(name);
            if candidate.is_dir() {
                push_unique_path(&mut found, candidate.clone());
                if let Ok(canonical) = candidate.canonicalize() {
                    push_unique_path(&mut found, canonical);
                }
            }
            if !current.pop() {
                break;
            }
        }
    }
    found
}

fn native_tsc_in(dir: &Path) -> Option<PathBuf> {
    let candidates = if cfg!(windows) {
        vec!["tsc.exe"]
    } else {
        vec!["tsc"]
    };
    candidates
        .into_iter()
        .map(|candidate| dir.join(candidate))
        .find(|candidate| candidate.is_file())
}

fn push_unique_path(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.iter().any(|path| path == &candidate) {
        paths.push(candidate);
    }
}

fn bin_in(dir: &Path, name: &str) -> Option<PathBuf> {
    let bin_dir = dir.join("node_modules").join(".bin");
    executable_candidates(name)?
        .into_iter()
        .map(|candidate| bin_dir.join(candidate))
        .find(|candidate| candidate.is_file())
}

fn executable_candidates(name: &str) -> Option<Vec<String>> {
    if !cfg!(windows) {
        return Some(vec![name.to_string()]);
    }

    let mut candidates = Vec::new();
    for extension in windows_path_extensions()? {
        candidates.push(format!("{name}{extension}"));
    }
    candidates.push(name.to_string());
    Some(candidates)
}

fn windows_path_extensions() -> Option<Vec<String>> {
    let value = std::env::var_os("PATHEXT")
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty())?;

    let mut extensions = Vec::new();
    for extension in value.split(';') {
        let extension = extension.trim();
        if extension.is_empty() {
            continue;
        }
        let extension = if extension.starts_with('.') {
            extension.to_string()
        } else {
            format!(".{extension}")
        };
        if !extensions
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&extension))
        {
            extensions.push(extension.to_ascii_lowercase());
        }
    }
    (!extensions.is_empty()).then_some(extensions)
}

fn typescript_platform() -> Option<&'static str> {
    match std::env::consts::OS {
        "aix" => Some("aix"),
        "freebsd" => Some("freebsd"),
        "linux" => Some("linux"),
        "macos" => Some("darwin"),
        "netbsd" => Some("netbsd"),
        "openbsd" => Some("openbsd"),
        "illumos" | "solaris" => Some("sunos"),
        "windows" => Some("win32"),
        _ => None,
    }
}

fn typescript_arch() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "aarch64" => Some("arm64"),
        "arm" => Some("arm"),
        "loongarch64" => Some("loong64"),
        "mips64" => Some("mips64el"),
        "powerpc64" => Some("ppc64"),
        "riscv64" => Some("riscv64"),
        "s390x" => Some("s390x"),
        "x86_64" => Some("x64"),
        _ => None,
    }
}
