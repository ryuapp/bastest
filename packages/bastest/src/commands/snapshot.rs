use std::fs;
use std::path::{Path, PathBuf};

pub fn execute(cwd: &Path, args: &[PathBuf]) -> i32 {
    let Some(action) = args.first().and_then(|arg| arg.to_str()) else {
        eprintln!("usage: bastest snapshot <accept|reject>");
        return 2;
    };

    let pending = match collect_pending_snapshots(cwd) {
        Ok(pending) => pending,
        Err(error) => {
            eprintln!("failed to collect pending snapshots: {error}");
            return 1;
        }
    };

    match action {
        "accept" => accept_pending(pending),
        "reject" => reject_pending(pending),
        _ => {
            eprintln!("usage: bastest snapshot <accept|reject>");
            2
        }
    }
}

fn accept_pending(pending: Vec<PathBuf>) -> i32 {
    let mut accepted = 0;
    for file in pending {
        let Some(target) = accepted_snapshot_path(&file) else {
            continue;
        };
        if let Err(error) = fs::rename(&file, &target) {
            eprintln!("failed to accept {}: {error}", display_path(&file));
            return 1;
        }
        accepted += 1;
    }
    println!("accepted {accepted} snapshots");
    0
}

fn reject_pending(pending: Vec<PathBuf>) -> i32 {
    let mut rejected = 0;
    for file in pending {
        if let Err(error) = fs::remove_file(&file) {
            eprintln!("failed to reject {}: {error}", display_path(&file));
            return 1;
        }
        rejected += 1;
    }
    println!("rejected {rejected} snapshots");
    0
}

fn collect_pending_snapshots(cwd: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut pending = Vec::new();
    walk(cwd, &mut pending)?;
    pending.sort();
    Ok(pending)
}

fn walk(dir: &Path, pending: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if matches!(name, ".git" | ".bastest" | "node_modules") {
                continue;
            }
            walk(&path, pending)?;
        } else if file_type.is_file() && is_pending_snapshot(&path) {
            pending.push(path);
        }
    }
    Ok(())
}

fn is_pending_snapshot(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".snap.new"))
}

fn accepted_snapshot_path(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let accepted = name.strip_suffix(".new")?;
    Some(path.with_file_name(accepted))
}

fn display_path(path: &Path) -> String {
    let value = path.display().to_string();
    if cfg!(windows) {
        value.replace('\\', "/")
    } else {
        value
    }
}
