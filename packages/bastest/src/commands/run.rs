use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use super::{
    config,
    reporter::{self, FileResult},
    transform::transform_test_file,
    typecheck,
};

const RESULT_PREFIX: &str = "__BASTEST_RESULT__";
const EVENT_PREFIX: &str = "__BASTEST_EVENT__";
const SHUTDOWN_MESSAGE: &str = "__BASTEST_SHUTDOWN__";
const EMBEDDED_NODE_RUNNER: &str = include_str!("../../dist/runners/node.mjs");

const TEST_EXTENSIONS: &[&str] = &["js", "mjs", "ts", "mts", "jsx", "tsx"];
const IGNORED_DIRS: &[&str] = &[".bastest"];
const DEFAULT_EXCLUDES: &[&str] = &[".git", "node_modules"];

pub struct RunOptions {
    pub cwd: PathBuf,
    pub package_root: PathBuf,
    pub agent: bool,
    pub discover_only: bool,
    pub filter: Option<String>,
    pub concurrency: Option<usize>,
    pub fail_fast: bool,
    pub files: Vec<PathBuf>,
}

pub fn execute(options: RunOptions) -> i32 {
    let runner = match materialize_node_runner(&options.cwd) {
        Ok(runner) => runner,
        Err(error) => {
            eprintln!("failed to write node runner: {error}");
            return 1;
        }
    };
    let node = env::var_os("BASTEST_RUNTIME_PATH").unwrap_or_else(|| OsString::from("node"));
    let config = match config::load(&options.cwd) {
        Ok(config) => config,
        Err(code) => return code,
    };

    let mut files: Vec<PathBuf> = options
        .files
        .into_iter()
        .map(|file| normalize_arg_path(file, &options.cwd))
        .collect();

    if files.is_empty() {
        files = match discover_tests(
            &options.cwd,
            config::exclude(&config),
            config::in_source_test_enabled(&config),
        ) {
            Ok(files) => files,
            Err(error) => {
                eprintln!("failed to discover tests: {error}");
                return 1;
            }
        };
    }

    if files.is_empty() {
        println!("No test files found.");
        return 0;
    }
    if options.discover_only {
        println!("{}", files.len());
        return 0;
    }

    if config::typecheck_enabled(&config) {
        let code = typecheck::run(
            &options.cwd,
            &options.package_root,
            config::typecheck_checker(&config),
            &files,
        );
        if code != 0 {
            return code;
        }
    }

    let agent = options.agent || config::agent_enabled(&config);
    let results = match run_files(RunFilesOptions {
        agent,
        cwd: options.cwd.clone(),
        package_root: options.package_root,
        runner,
        node,
        filter: options.filter.or(config.filter),
        concurrency: options.concurrency.or(config.concurrency),
        fail_fast: options.fail_fast || config.fail_fast.unwrap_or(false),
        files,
    }) {
        Ok(results) => results,
        Err(code) => return code,
    };

    let failed = count_failed(&results);
    let summary = if agent && failed > 0 {
        reporter::report_summary_compact(&results)
    } else {
        reporter::report_summary(&results)
    };
    if agent && failed > 0 {
        match write_agent_report(&options.cwd, &summary, &results) {
            Ok(report_dir) => println!("Report Dir: {}", display_path(&report_dir)),
            Err(error) => eprintln!("failed to write bastest report: {error}"),
        }
    }
    if agent {
        for result in &results {
            reporter::report_file(result, &options.cwd, true);
        }
    }
    if failed > 0 { 1 } else { 0 }
}

fn count_failed(results: &[FileResult]) -> usize {
    results
        .iter()
        .map(|file| {
            usize::from(file.load_error.is_some())
                + file
                    .tests
                    .iter()
                    .filter(|test| test.status == reporter::TestStatus::Failed)
                    .count()
        })
        .sum()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentReport<'a> {
    summary: &'a reporter::Summary,
    failures: Vec<&'a FileResult>,
}

fn write_agent_report(
    cwd: &Path,
    summary: &reporter::Summary,
    results: &[FileResult],
) -> std::io::Result<PathBuf> {
    let report_dir = cwd.join(".bastest").join("reports").join("latest");
    let files_dir = report_dir.join("files");
    let _ = fs::remove_dir_all(&report_dir);
    fs::create_dir_all(&files_dir)?;

    let failures = results
        .iter()
        .filter(|file| file.load_error.is_some() || has_failed_test(&file.tests))
        .collect::<Vec<_>>();

    write_json(&report_dir.join("summary.json"), summary)?;
    write_json(
        &report_dir.join("failures.json"),
        &AgentReport {
            summary,
            failures: failures.clone(),
        },
    )?;

    for file in failures {
        write_json(&files_dir.join(report_file_name(&file.file)), file)?;
    }

    Ok(report_dir)
}

fn display_path(path: &Path) -> String {
    let value = path.display().to_string();
    if cfg!(windows) {
        value.replace('\\', "/")
    } else {
        value
    }
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let content = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    fs::write(path, format!("{content}\n"))
}

fn report_file_name(file: &str) -> String {
    let name = file.trim_start_matches('/').replace(['\\', '/', ':'], "__");
    format!("{name}.json")
}

fn has_failed_test(tests: &[reporter::TestResult]) -> bool {
    tests.iter().any(|test| {
        test.status == reporter::TestStatus::Failed || has_failed_step_for_report(&test.steps)
    })
}

fn has_failed_step_for_report(steps: &[reporter::StepResult]) -> bool {
    steps.iter().any(|step| {
        step.status == reporter::TestStatus::Failed || has_failed_step_for_report(&step.steps)
    })
}

struct RunFilesOptions {
    agent: bool,
    cwd: PathBuf,
    package_root: PathBuf,
    runner: PathBuf,
    node: OsString,
    filter: Option<String>,
    concurrency: Option<usize>,
    fail_fast: bool,
    files: Vec<PathBuf>,
}

fn run_files(options: RunFilesOptions) -> Result<Vec<FileResult>, i32> {
    let concurrency = options
        .concurrency
        .unwrap_or_else(default_concurrency)
        .max(1)
        .min(options.files.len().max(1));
    let queue = Arc::new(Mutex::new(options.files));
    let results = Arc::new(Mutex::new(Vec::new()));
    let failed = Arc::new(AtomicBool::new(false));
    let fatal = Arc::new(Mutex::new(None));
    let output = Arc::new(Mutex::new(()));

    let mut threads = Vec::new();
    for _ in 0..concurrency {
        let worker = NodeWorker::new(NodeWorkerConfig {
            node: options.node.clone(),
            runner: options.runner.clone(),
            cwd: options.cwd.clone(),
            package_root: options.package_root.clone(),
            filter: options.filter.clone(),
        })?;
        let queue = Arc::clone(&queue);
        let results = Arc::clone(&results);
        let failed = Arc::clone(&failed);
        let fatal = Arc::clone(&fatal);
        let output = Arc::clone(&output);
        let fail_fast = options.fail_fast;
        let cwd = options.cwd.clone();
        let agent = options.agent;
        let package_root = options.package_root.clone();

        threads.push(thread::spawn(move || {
            loop {
                if fail_fast && failed.load(Ordering::Relaxed) {
                    return;
                }

                let file = {
                    let mut queue = queue.lock().expect("test queue lock poisoned");
                    queue.pop()
                };
                let Some(file) = file else {
                    return;
                };

                let bundle_file = match transform_test_file(&file, &package_root) {
                    Ok(bundle_file) => bundle_file,
                    Err(error) => {
                        eprintln!("{error}");
                        *fatal.lock().expect("fatal lock poisoned") = Some(1);
                        failed.store(true, Ordering::Relaxed);
                        return;
                    }
                };
                let result = match worker.run_file(&file, &bundle_file, &cwd, agent, &output) {
                    Ok(result) => result,
                    Err(code) => {
                        *fatal.lock().expect("fatal lock poisoned") = Some(code);
                        failed.store(true, Ordering::Relaxed);
                        return;
                    }
                };
                let result_failed = result.load_error.is_some()
                    || result
                        .tests
                        .iter()
                        .any(|test| test.status == reporter::TestStatus::Failed);
                {
                    if !agent && result.load_error.is_some() {
                        let _output = output.lock().expect("output lock poisoned");
                        reporter::report_file(&result, &cwd, agent);
                    }
                }
                if result_failed {
                    failed.store(true, Ordering::Relaxed);
                }
                results.lock().expect("results lock poisoned").push(result);
            }
        }));
    }

    for thread in threads {
        let _ = thread.join();
    }

    if let Some(code) = *fatal.lock().expect("fatal lock poisoned") {
        return Err(code);
    }

    let mut results = Arc::try_unwrap(results)
        .expect("results still shared")
        .into_inner()
        .expect("results lock poisoned");
    results.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(results)
}

struct NodeWorkerConfig {
    node: OsString,
    runner: PathBuf,
    cwd: PathBuf,
    package_root: PathBuf,
    filter: Option<String>,
}

struct NodeWorker {
    process: Mutex<NodeWorkerProcess>,
    config: NodeWorkerConfig,
}

struct NodeWorkerProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerRequest<'a> {
    file: &'a Path,
    bundle_file: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerEvent {
    #[serde(rename = "type")]
    event_type: String,
    file: PathBuf,
    test: reporter::TestResult,
}

impl NodeWorker {
    fn new(config: NodeWorkerConfig) -> Result<Self, i32> {
        let mut child = Command::new(&config.node)
            .arg(&config.runner)
            .arg("--cwd")
            .arg(&config.cwd)
            .arg("--worker")
            .current_dir(&config.package_root)
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap_or_else(|error| {
                eprintln!("failed to start node worker: {error}");
                std::process::exit(2);
            });
        let stdin = child.stdin.take().ok_or_else(|| {
            cleanup_child(&mut child);
            eprintln!("failed to open node worker stdin");
            1
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            cleanup_child(&mut child);
            eprintln!("failed to open node worker stdout");
            1
        })?;
        Ok(Self {
            process: Mutex::new(NodeWorkerProcess {
                child,
                stdin: Some(stdin),
                stdout: BufReader::new(stdout),
            }),
            config,
        })
    }

    fn run_file(
        &self,
        file: &Path,
        bundle_file: &Path,
        cwd: &Path,
        agent: bool,
        output: &Arc<Mutex<()>>,
    ) -> Result<FileResult, i32> {
        let request = WorkerRequest {
            file,
            bundle_file: path_to_file_url(bundle_file),
            stream: !agent,
            filter: self.config.filter.as_ref(),
        };
        let payload = serde_json::to_string(&request).map_err(|error| {
            eprintln!("failed to serialize worker request: {error}");
            1
        })?;
        let mut process = self.process.lock().expect("node worker lock poisoned");
        let stdin = process.stdin.as_mut().ok_or_else(|| {
            eprintln!("node worker stdin is closed");
            1
        })?;
        writeln!(stdin, "{payload}").map_err(|error| {
            eprintln!("failed to write to node worker: {error}");
            1
        })?;
        stdin.flush().map_err(|error| {
            eprintln!("failed to flush node worker request: {error}");
            1
        })?;

        let mut line = String::new();
        let mut file_printed = false;
        loop {
            line.clear();
            let bytes = process.stdout.read_line(&mut line).map_err(|error| {
                eprintln!("failed to read node worker output: {error}");
                1
            })?;
            if bytes == 0 {
                eprintln!("node worker exited before returning test result");
                return Err(1);
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(payload) = line.strip_prefix(RESULT_PREFIX) {
                return serde_json::from_str(payload).map_err(|error| {
                    eprintln!("failed to parse test results: {error}");
                    1
                });
            } else if let Some(payload) = line.strip_prefix(EVENT_PREFIX) {
                let event = serde_json::from_str::<WorkerEvent>(payload).map_err(|error| {
                    eprintln!("failed to parse test event: {error}");
                    1
                })?;
                if !agent && event.event_type == "test" {
                    let _output = output.lock().expect("output lock poisoned");
                    reporter::report_stream_test(
                        &event.file,
                        cwd,
                        &event.test,
                        agent,
                        &mut file_printed,
                    );
                }
            } else {
                let _output = output.lock().expect("output lock poisoned");
                println!("{line}");
            }
        }
    }
}

impl Drop for NodeWorker {
    fn drop(&mut self) {
        if let Ok(mut process) = self.process.lock() {
            if let Some(mut stdin) = process.stdin.take() {
                let _ = writeln!(stdin, "{SHUTDOWN_MESSAGE}");
                let _ = stdin.flush();
            }
            let _ = process.child.wait();
        }
    }
}

fn cleanup_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn default_concurrency() -> usize {
    thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
}

fn materialize_node_runner(cwd: &Path) -> std::io::Result<PathBuf> {
    let runner = cwd
        .join(".bastest")
        .join("runner")
        .join(env!("CARGO_PKG_VERSION"))
        .join("node.mjs");
    let should_write = match fs::read_to_string(&runner) {
        Ok(current) => current != EMBEDDED_NODE_RUNNER,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => return Err(error),
    };
    if should_write {
        if let Some(parent) = runner.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&runner, EMBEDDED_NODE_RUNNER)?;
    }
    Ok(runner)
}

fn path_to_file_url(path: &Path) -> String {
    url::Url::from_file_path(path)
        .expect("bundle file path should be absolute")
        .to_string()
}

fn normalize_arg_path(path: PathBuf, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        return path;
    }

    let resolved = cwd.join(&path);
    if resolved.exists() {
        return resolved;
    }

    path
}

fn discover_tests(
    cwd: &Path,
    exclude: Option<&[String]>,
    in_source_test: bool,
) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let excludes = Excludes::new(exclude);
    walk_tests(cwd, cwd, &excludes, &mut files)?;
    let source_dir = cwd.join("src");
    if source_dir.is_dir() {
        walk_sources(cwd, &source_dir, &excludes, &mut files, in_source_test)?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

struct Excludes {
    patterns: Vec<String>,
}

impl Excludes {
    fn new(configured: Option<&[String]>) -> Self {
        let patterns = configured
            .map(|patterns| {
                patterns
                    .iter()
                    .map(|pattern| normalize_pattern(pattern))
                    .collect()
            })
            .unwrap_or_else(|| {
                DEFAULT_EXCLUDES
                    .iter()
                    .map(|pattern| (*pattern).to_string())
                    .collect()
            });
        Self { patterns }
    }

    fn matches(&self, cwd: &Path, path: &Path) -> bool {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        let relative = path
            .strip_prefix(cwd)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        self.patterns.iter().any(|pattern| {
            file_name == pattern
                || relative == *pattern
                || relative
                    .strip_prefix(pattern)
                    .is_some_and(|rest| rest.starts_with('/'))
                || glob_match(pattern, &relative)
        })
    }
}

fn walk_tests(
    cwd: &Path,
    dir: &Path,
    excludes: &Excludes,
    files: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if excludes.matches(cwd, &path) {
            continue;
        }
        if file_type.is_dir() {
            if let Some(name) = path.file_name().and_then(|name| name.to_str())
                && IGNORED_DIRS.contains(&name)
            {
                continue;
            }
            walk_tests(cwd, &path, excludes, files)?;
        } else if file_type.is_file() && is_test_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn walk_sources(
    cwd: &Path,
    dir: &Path,
    excludes: &Excludes,
    files: &mut Vec<PathBuf>,
    in_source_test: bool,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if excludes.matches(cwd, &path) {
            continue;
        }
        if file_type.is_dir() {
            if let Some(name) = path.file_name().and_then(|name| name.to_str())
                && IGNORED_DIRS.contains(&name)
            {
                continue;
            }
            walk_sources(cwd, &path, excludes, files, in_source_test)?;
        } else if file_type.is_file() {
            push_discovered_file(&path, files, in_source_test)?;
        }
    }
    Ok(())
}

fn push_discovered_file(
    path: &Path,
    files: &mut Vec<PathBuf>,
    in_source_test: bool,
) -> std::io::Result<()> {
    if is_test_file(path)
        || (in_source_test && is_source_file(path) && contains_in_source_test(path)?)
    {
        files.push(path.to_path_buf());
    }
    Ok(())
}

fn is_test_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    if !TEST_EXTENSIONS.contains(&extension) {
        return false;
    }

    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    stem.ends_with("_test") || stem.ends_with(".test")
}

fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| TEST_EXTENSIONS.contains(&extension))
}

fn contains_in_source_test(path: &Path) -> std::io::Result<bool> {
    const NEEDLE: &[u8] = b"import.meta.test";
    let source = fs::read(path)?;
    Ok(source.windows(NEEDLE.len()).any(|window| window == NEEDLE))
}

fn normalize_pattern(pattern: &str) -> String {
    pattern
        .trim_start_matches("./")
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern.split('/').collect::<Vec<_>>();
    let path = path.split('/').collect::<Vec<_>>();
    glob_segments_match(&pattern, &path)
}

fn glob_segments_match(pattern: &[&str], path: &[&str]) -> bool {
    let Some((head, tail)) = pattern.split_first() else {
        return path.is_empty();
    };

    if *head == "**" {
        return glob_segments_match(tail, path)
            || (!path.is_empty() && glob_segments_match(pattern, &path[1..]));
    }

    let Some((path_head, path_tail)) = path.split_first() else {
        return false;
    };
    wildcard_segment_match(head, path_head) && glob_segments_match(tail, path_tail)
}

fn wildcard_segment_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut pattern_index, mut value_index) = (0, 0);
    let (mut star_index, mut star_value_index) = (None, 0);

    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_value_index = value_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_value_index += 1;
            value_index = star_value_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}
