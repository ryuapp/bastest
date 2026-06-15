use std::fs;
use std::path::Path;

pub fn execute(cwd: &Path) -> i32 {
    let state_dir = cwd.join(".bastest");
    let _ = fs::remove_dir_all(&state_dir);
    println!("Removed .bastest");
    0
}
