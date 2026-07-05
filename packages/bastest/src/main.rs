mod cli;
mod commands;

use std::env;
use std::path::PathBuf;
use std::process::exit;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let package_root = match find_package_root(&cwd) {
        Some(package_root) => package_root,
        None => {
            eprintln!("failed to find bastest package root");
            exit(2);
        }
    };
    let project_root = match find_project_root(&cwd) {
        Some(project_root) => project_root,
        None => {
            eprintln!(
                "could not find `bastest.jsonc` in `{}` or any parent directory",
                cwd.display()
            );
            exit(2);
        }
    };

    let cli = cli::parse();
    let code = match cli.command {
        cli::CommandKind::Run => commands::run::execute(commands::run::RunOptions {
            cwd: project_root,
            package_root,
            agent: cli.agent,
            discover_only: cli.discover_only,
            filter: cli.filter,
            concurrency: cli.concurrency,
            fail_fast: cli.fail_fast,
            files: cli.files,
        }),
        cli::CommandKind::Clean => commands::clean::execute(&project_root),
        cli::CommandKind::Snapshot => commands::snapshot::execute(&project_root, &cli.files),
        cli::CommandKind::Watch => commands::watch::execute(),
    };

    exit(code);
}

fn find_package_root(cwd: &std::path::Path) -> Option<PathBuf> {
    if let Some(package_root) = env::var_os("BASTEST_PACKAGE_ROOT").map(PathBuf::from)
        && is_package_root(&package_root)
    {
        return Some(package_root);
    }

    if is_package_root(cwd) {
        return Some(cwd.to_path_buf());
    }

    let root = find_workspace_root(cwd)?;
    let package_root = root.join("packages").join("bastest");
    is_package_root(&package_root).then_some(package_root)
}

fn find_workspace_root(start: &std::path::Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current
            .join("packages")
            .join("bastest")
            .join("package.json")
            .is_file()
        {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn find_project_root(start: &std::path::Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join("bastest.jsonc").is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn is_package_root(path: &std::path::Path) -> bool {
    path.join("package.json").is_file()
        && (path.join("dist").join("index.mjs").is_file()
            || path.join("src").join("mod.ts").is_file())
}
