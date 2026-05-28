//! End-to-end integration tests that exercise oci-interceptor through the Docker daemon.
//!
//! These tests are gated by the `OCI_INTERCEPTOR_INTEGRATION` env var so `cargo test` stays
//! portable on developer machines without Docker. They require the Docker daemon to be
//! configured with a specific set of named runtimes; see the `integration` job in
//! `.github/workflows/CI.yml` for the daemon.json contents.
//!
//! Required runtime names:
//! - `oi-default`        — interceptor with no flags
//! - `oi-ro-net`         — `--oi-readonly-networking-mounts`
//! - `oi-env-foo`        — `--oi-env FOO=bar`
//! - `oi-env-force-foo`  — `--oi-env-force FOO=forced`
//! - `oi-debug`          — `--oi-write-debug-output --oi-debug-output-dir <DEBUG_DIR>`
//! - `oi-debug-ro-net`   — same as `oi-debug` plus `--oi-readonly-networking-mounts`

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_IMAGE: &str = "alpine:3.20";
const DEBUG_DIR: &str = "/tmp/oi-interceptor-debug";

fn check_enabled(test_name: &str) -> bool {
    if std::env::var("OCI_INTERCEPTOR_INTEGRATION").is_ok() {
        return true;
    }
    eprintln!("skipping {test_name}: set OCI_INTERCEPTOR_INTEGRATION=1 to run");
    false
}

fn unique_hostname(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{nanos}-{n}")
}

fn docker_run(runtime: &str, extra_args: &[&str], cmd: &[&str]) -> Output {
    let mut args: Vec<&str> = vec!["run", "--rm", "--runtime", runtime];
    args.extend(extra_args);
    args.push(TEST_IMAGE);
    args.extend(cmd);
    let output = Command::new("docker")
        .args(&args)
        .output()
        .expect("failed to invoke `docker`; is the daemon running?");
    if !output.status.success() {
        eprintln!(
            "docker run failed (status {:?})\nargs: {:?}\nstdout: {}\nstderr: {}",
            output.status.code(),
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    output
}

fn mount_options(runtime: &str, target: &str) -> String {
    let script = format!(r#"awk '$2 == "{target}" {{ print $4; exit }}' /proc/mounts"#);
    let out = docker_run(runtime, &[], &["sh", "-c", &script]);
    assert!(
        out.status.success(),
        "failed to read /proc/mounts for {target}"
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn passthrough_runs_container() {
    if !check_enabled("passthrough_runs_container") {
        return;
    }
    let out = docker_run("oi-default", &[], &["echo", "hello"]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
}

#[test]
fn passthrough_propagates_nonzero_exit_code() {
    if !check_enabled("passthrough_propagates_nonzero_exit_code") {
        return;
    }
    let out = docker_run("oi-default", &[], &["sh", "-c", "exit 42"]);
    assert_eq!(out.status.code(), Some(42));
}

#[test]
fn networking_mounts_writable_without_flag() {
    if !check_enabled("networking_mounts_writable_without_flag") {
        return;
    }
    for target in ["/etc/hosts", "/etc/hostname", "/etc/resolv.conf"] {
        let opts = mount_options("oi-default", target);
        let opts: Vec<&str> = opts.split(',').collect();
        assert!(
            opts.contains(&"rw"),
            "{target} expected to be rw by default, got: {opts:?}"
        );
    }
}

#[test]
fn networking_mounts_readonly_with_flag() {
    if !check_enabled("networking_mounts_readonly_with_flag") {
        return;
    }
    for target in ["/etc/hosts", "/etc/hostname", "/etc/resolv.conf"] {
        let opts = mount_options("oi-ro-net", target);
        let opts: Vec<&str> = opts.split(',').collect();
        assert!(
            opts.contains(&"ro"),
            "{target} expected to be ro with --oi-readonly-networking-mounts, got: {opts:?}"
        );
    }
}

#[test]
fn networking_mounts_writes_blocked_with_flag() {
    if !check_enabled("networking_mounts_writes_blocked_with_flag") {
        return;
    }
    for target in ["/etc/hosts", "/etc/hostname", "/etc/resolv.conf"] {
        let script = format!("exec 2>&1; echo x > {target}; echo exit=$?");
        let out = docker_run("oi-ro-net", &[], &["sh", "-c", &script]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.to_lowercase().contains("read-only"),
            "writing {target} should have produced a read-only error, got:\n{stdout}"
        );
        assert!(
            !stdout.contains("exit=0"),
            "writing {target} should have failed, got:\n{stdout}"
        );
    }
}

#[test]
fn oi_env_sets_default_when_unset() {
    if !check_enabled("oi_env_sets_default_when_unset") {
        return;
    }
    let out = docker_run("oi-env-foo", &[], &["sh", "-c", "echo FOO=$FOO"]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "FOO=bar");
}

#[test]
fn oi_env_does_not_override_user_value() {
    if !check_enabled("oi_env_does_not_override_user_value") {
        return;
    }
    let out = docker_run(
        "oi-env-foo",
        &["-e", "FOO=user"],
        &["sh", "-c", "echo FOO=$FOO"],
    );
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "FOO=user");
}

#[test]
fn oi_env_force_overrides_user_value() {
    if !check_enabled("oi_env_force_overrides_user_value") {
        return;
    }
    let out = docker_run(
        "oi-env-force-foo",
        &["-e", "FOO=user"],
        &["sh", "-c", "echo FOO=$FOO"],
    );
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "FOO=forced");
}

#[test]
fn oi_env_force_sets_when_unset() {
    if !check_enabled("oi_env_force_sets_when_unset") {
        return;
    }
    let out = docker_run("oi-env-force-foo", &[], &["sh", "-c", "echo FOO=$FOO"]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "FOO=forced");
}

#[test]
fn debug_output_files_written() {
    if !check_enabled("debug_output_files_written") {
        return;
    }
    let hostname = unique_hostname("oi-debug-test");
    let out = docker_run("oi-debug", &["--hostname", &hostname], &["true"]);
    assert!(out.status.success());

    let debug_dir = PathBuf::from(DEBUG_DIR);
    let original = debug_dir.join(format!("{hostname}_original.json"));
    let parsed = debug_dir.join(format!("{hostname}_parsed.json"));
    let calls_log = debug_dir.join("runtime_calls.log");

    assert!(
        original.exists(),
        "expected {} to exist",
        original.display()
    );
    assert!(parsed.exists(), "expected {} to exist", parsed.display());
    assert!(
        calls_log.exists(),
        "expected {} to exist",
        calls_log.display()
    );

    let parsed_contents = std::fs::read_to_string(&parsed).expect("debug parsed file unreadable");
    assert!(
        parsed_contents.contains(&hostname),
        "parsed config didn't contain hostname {hostname}"
    );
}

#[test]
fn debug_output_modified_file_written_when_spec_changes() {
    if !check_enabled("debug_output_modified_file_written_when_spec_changes") {
        return;
    }
    let hostname = unique_hostname("oi-debug-mod-test");
    let out = docker_run("oi-debug-ro-net", &["--hostname", &hostname], &["true"]);
    assert!(out.status.success());

    let modified = PathBuf::from(DEBUG_DIR).join(format!("{hostname}_modified.json"));
    assert!(
        modified.exists(),
        "expected {} to exist",
        modified.display()
    );
}
