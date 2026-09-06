//! Tests for how oci-interceptor hands off to the underlying OCI runtime.
//!
//! These use a stub runtime script rather than runc, so they need neither Docker nor a
//! real runtime and always run as part of `cargo test`.
//!
//! The property under test is that the interceptor *execs* the runtime rather than
//! spawning it as a child and waiting. containerd's shim invokes the runtime through
//! go-runc, which builds commands with Go's `exec.CommandContext`; a cancelled call
//! delivers SIGKILL to the direct child only. If the interceptor were still in the
//! process tree at that point, the signal would kill the interceptor and orphan the real
//! runtime underneath it.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BIN: &str = env!("CARGO_BIN_EXE_oci-interceptor");

fn unique_dir(prefix: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("oi-{prefix}-{nanos}-{n}"));
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir
}

/// Writes a stub runtime that records its own PID and then blocks, so the test can
/// inspect the process tree while a runtime call is in flight.
fn write_stub_runtime(dir: &Path, pid_file: &Path) -> PathBuf {
    let path = dir.join("stub-runtime");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\necho $$ > {}\nexec sleep 30\n",
            pid_file.display()
        ),
    )
    .expect("failed to write stub runtime");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod stub runtime");
    path
}

fn wait_for_pid_file(pid_file: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(contents) = fs::read_to_string(pid_file)
            && let Ok(pid) = contents.trim().parse::<u32>()
        {
            return pid;
        }
        sleep(Duration::from_millis(20));
    }
    panic!(
        "stub runtime never recorded its PID at {}",
        pid_file.display()
    );
}

fn process_alive(pid: u32) -> bool {
    // `kill -0` rather than a /proc lookup: this module is cfg(unix), and on a Unix
    // without procfs the path test reports every process as dead, which would turn the
    // orphan assertion below into a green test that checks nothing. Panic rather than
    // defaulting to false, for the same reason.
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        // kill(1) writes "No such process" to stderr for a dead pid, which is the normal
        // result here and only clutters the test output.
        .stderr(Stdio::null())
        .status()
        .expect("failed to run kill -0")
        .success()
}

/// The interceptor must exec the runtime, leaving the runtime with the interceptor's own
/// PID. If it spawned and waited instead, the runtime would report a different PID.
#[test]
fn execs_runtime_in_place_rather_than_spawning_a_child() {
    let dir = unique_dir("exec");
    let pid_file = dir.join("runtime.pid");
    let stub = write_stub_runtime(&dir, &pid_file);

    let mut child = Command::new(BIN)
        .arg("--oi-runtime-path")
        .arg(&stub)
        .args(["state", "some-container-id"])
        .spawn()
        .expect("failed to invoke oci-interceptor");

    let runtime_pid = wait_for_pid_file(&pid_file);
    let interceptor_pid = child.id();

    let _ = child.kill();
    let _ = child.wait();
    // When this assertion is about to fail the runtime is a separate process, so killing
    // the interceptor leaves it behind. Clean up before reporting.
    if runtime_pid != interceptor_pid {
        force_kill(runtime_pid);
    }
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        runtime_pid, interceptor_pid,
        "runtime ran as PID {runtime_pid} but the interceptor was PID {interceptor_pid}; \
         the interceptor spawned the runtime as a child instead of exec'ing it, which \
         orphans the runtime when the caller signals the interceptor"
    );
}

/// Signalling the interceptor must take the runtime down with it. A spawn-and-wait
/// wrapper leaves the runtime running after the wrapper is killed.
#[test]
fn signalling_the_interceptor_does_not_orphan_the_runtime() {
    let dir = unique_dir("orphan");
    let pid_file = dir.join("runtime.pid");
    let stub = write_stub_runtime(&dir, &pid_file);

    let mut child = Command::new(BIN)
        .arg("--oi-runtime-path")
        .arg(&stub)
        .args(["delete", "some-container-id"])
        .spawn()
        .expect("failed to invoke oci-interceptor");

    let runtime_pid = wait_for_pid_file(&pid_file);

    // SIGKILL to the process the caller started, exactly as go-runc's
    // exec.CommandContext does when a runtime call is cancelled.
    child.kill().expect("failed to kill oci-interceptor");
    child.wait().expect("failed to reap oci-interceptor");

    // Give the kernel a moment to tear the process down before checking.
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(runtime_pid) && Instant::now() < deadline {
        sleep(Duration::from_millis(20));
    }
    let orphaned = process_alive(runtime_pid);

    if orphaned {
        force_kill(runtime_pid);
    }
    let _ = fs::remove_dir_all(&dir);

    assert!(
        !orphaned,
        "runtime PID {runtime_pid} survived the interceptor being killed; a cancelled \
         runtime call would leave an orphaned runc behind, and its shim never shut down"
    );
}

/// Best-effort SIGKILL so a failing test does not leak the stub runtime.
fn force_kill(pid: u32) {
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
}

/// Writes a stub runtime that exits immediately in a given way.
fn write_stub(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        format!(
            "#!/bin/sh
{body}
"
        ),
    )
    .expect("failed to write stub");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("failed to chmod stub");
    path
}

/// A runtime that cannot be exec'd must surface as a non-zero exit naming the path, rather
/// than being mistaken for a runtime that ran and failed.
#[test]
fn failure_to_exec_the_runtime_is_reported() {
    let out = Command::new(BIN)
        .arg("--oi-runtime-path")
        .arg("/nonexistent/runc")
        .args(["state", "some-container-id"])
        .output()
        .expect("failed to invoke oci-interceptor");

    assert!(!out.status.success(), "expected a non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("/nonexistent/runc"),
        "stderr should name the runtime it could not execute, got: {stderr}"
    );
}

/// A runtime killed by a signal must be reported as signal death. Spawning and waiting
/// reported it as exit code -1, which process::exit truncates to 255 — indistinguishable
/// from a runtime that genuinely exited 255.
#[test]
fn a_signal_killed_runtime_is_not_reported_as_exit_255() {
    let dir = unique_dir("signal");
    let stub = write_stub(&dir, "suicidal-runtime", "kill -TERM $$");

    let status = Command::new(BIN)
        .arg("--oi-runtime-path")
        .arg(&stub)
        .args(["state", "some-container-id"])
        .status()
        .expect("failed to invoke oci-interceptor");

    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        status.signal(),
        Some(15),
        "expected death by SIGTERM, got {status:?}"
    );
    assert_eq!(
        status.code(),
        None,
        "a signal-killed runtime must not present as an ordinary exit code"
    );
}

/// Exit codes must survive the exec. Covered by the integration suite too, but that is
/// gated behind OCI_INTERCEPTOR_INTEGRATION and silently passes without Docker.
#[test]
fn runtime_exit_code_propagates() {
    let dir = unique_dir("exitcode");
    let stub = write_stub(&dir, "exit42-runtime", "exit 42");

    let status = Command::new(BIN)
        .arg("--oi-runtime-path")
        .arg(&stub)
        .args(["state", "some-container-id"])
        .status()
        .expect("failed to invoke oci-interceptor");

    let _ = fs::remove_dir_all(&dir);
    assert_eq!(status.code(), Some(42));
}
