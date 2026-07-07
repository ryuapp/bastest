use bpaf::{Parser, construct, long, positional};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Cli {
    pub agent: bool,
    pub discover_only: bool,
    pub filter: Option<String>,
    pub concurrency: Option<usize>,
    pub fail_fast: bool,
    pub command: CommandKind,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum CommandKind {
    Run,
    Clean,
    Snapshot,
    Watch,
}

pub fn parse() -> Cli {
    parser().run()
}

fn parser() -> bpaf::OptionParser<Cli> {
    let agent = long("agent")
        .help("Use concise AI-agent-oriented test output")
        .switch();
    let discover_only = long("discover-only")
        .help("Only print discovered test file count")
        .switch();
    let filter = long("filter")
        .help("Only run tests matching a pattern")
        .argument::<String>("PATTERN")
        .optional();
    let concurrency = long("concurrency")
        .help("Maximum number of test files to run concurrently")
        .argument::<usize>("COUNT")
        .optional();
    let fail_fast = long("fail-fast")
        .help("Stop after the first failed test file")
        .switch();
    let files = positional::<PathBuf>("FILE").many();

    construct!(Cli {
        agent,
        discover_only,
        filter,
        concurrency,
        fail_fast,
        command(),
        files
    })
    .to_options()
    .version(env!("CARGO_PKG_VERSION"))
}

fn command() -> impl Parser<CommandKind> {
    bpaf::any::<String, _, _>("COMMAND", |value| match value.as_str() {
        "run" => Some(CommandKind::Run),
        "clean" => Some(CommandKind::Clean),
        "snapshot" => Some(CommandKind::Snapshot),
        "watch" => Some(CommandKind::Watch),
        _ => None,
    })
    .optional()
    .map(|command| command.unwrap_or(CommandKind::Run))
}
