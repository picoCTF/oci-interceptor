//! CLI smoke tests for the oci-interceptor binary.
//!
//! These exercise clap-handled flags that short-circuit before any runtime call,
//! so they do not require Docker or runc and always run as part of `cargo test`.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_oci-interceptor");

#[test]
fn version_flag_prints_version() {
    let out = Command::new(BIN)
        .arg("--oi-version")
        .output()
        .expect("failed to invoke oci-interceptor");
    assert!(
        out.status.success(),
        "--oi-version exited non-zero: {:?}",
        out.status
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "version output missing crate version: {stdout}"
    );
}

#[test]
fn help_flag_prints_usage() {
    let out = Command::new(BIN)
        .arg("--oi-help")
        .output()
        .expect("failed to invoke oci-interceptor");
    assert!(
        out.status.success(),
        "--oi-help exited non-zero: {:?}",
        out.status
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--oi-runtime-path"),
        "help output missing expected flag, got: {stdout}"
    );
}
