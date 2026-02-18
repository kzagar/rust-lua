use std::process::Command;

fn main() {
    let version = env!("CARGO_PKG_VERSION");
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "staging".to_string());

    let build_env = std::env::var("MLUA_TEST_BUILD_ENV").unwrap_or_else(|_| "staging".to_string());

    println!("cargo:rustc-env=MLUA_TEST_VERSION={}-{}", version, git_hash);
    println!("cargo:rustc-env=MLUA_TEST_BUILD_ENV={}", build_env);
    println!("cargo:rerun-if-env-changed=MLUA_TEST_BUILD_ENV");
    println!("cargo:rerun-if-changed=.git/HEAD");
}
