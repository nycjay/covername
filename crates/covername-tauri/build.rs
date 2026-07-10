use std::process::Command;

fn main() {
    tauri_build::build();

    // Capture git hash at build time for version display
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();

    if let Ok(output) = output {
        let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !hash.is_empty() {
            println!("cargo:rustc-env=COVERNAME_GIT_HASH={hash}");
        }
    }

    // Rebuild if git HEAD changes (new commits)
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
