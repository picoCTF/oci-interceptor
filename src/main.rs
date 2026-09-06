mod env_vars;
mod networking_mounts;

use anyhow::{Context, Result};
use clap::{
    Arg, ArgAction, ValueHint, crate_authors, crate_description, crate_name, crate_version,
};
use env_vars::{EnvVar, EnvVarOverride, modify_env_vars, parse_env_var};
use networking_mounts::modify_networking_mounts;
use oci_spec::runtime::Spec;
use std::{fs, io::Write, os::unix::process::CommandExt, path::PathBuf, process};

fn main() -> Result<()> {
    let matches = clap::Command::new(crate_name!())
        .version(crate_version!())
        .disable_version_flag(true)
        .disable_help_flag(true)
        .author(crate_authors!())
        .about(crate_description!())
        .dont_delimit_trailing_values(true)
        .arg(
            Arg::new("runtime-path")
                .long("oi-runtime-path")
                .value_hint(ValueHint::ExecutablePath)
                .default_value("runc")
                .help("Path to OCI runtime."),
        )
        .arg(
            // Retained as an accepted no-op so a daemon.json still passing it keeps
            // working: rejecting it would fail every container launch on any host
            // upgraded before its runtime config was updated.
            Arg::new("readonly-networking-mounts")
                .long("oi-readonly-networking-mounts")
                .action(ArgAction::SetTrue)
                .help("Deprecated, and now a no-op: networking mounts are always readonly"),
        )
        .arg(
            Arg::new("write-debug-output")
                .long("oi-write-debug-output")
                .action(ArgAction::SetTrue)
                .help("Write debug output"),
        )
        .arg(
            Arg::new("debug-output-dir")
                .long("oi-debug-output-dir")
                .value_hint(ValueHint::DirPath)
                .default_value("/var/log/oci-interceptor")
                .help("Debug output location"),
        )
        .arg(
            Arg::new("env-vars")
                .long("oi-env")
                .action(ArgAction::Append)
                .value_name("NAME=VALUE")
                .value_parser(parse_env_var)
                .help("Set an environment variable if not already present in config"),
        )
        .arg(
            Arg::new("env-vars-forced")
                .long("oi-env-force")
                .action(ArgAction::Append)
                .value_name("NAME=VALUE")
                .value_parser(parse_env_var)
                .help("Override an environment variable, regardless of any original value"),
        )
        .arg(
            Arg::new("version")
                .long("oi-version")
                .action(ArgAction::Version)
                .help("Print version"),
        )
        .arg(
            Arg::new("help")
                .long("oi-help")
                .action(ArgAction::Help)
                .help("Print help"),
        )
        .arg(
            Arg::new("runtime-options")
                .action(ArgAction::Append)
                .trailing_var_arg(true)
                .allow_hyphen_values(true)
                .help("All additional options will be forwarded to the OCI runtime."),
        )
        .get_matches();

    let runtime_path = matches
        .get_one::<String>("runtime-path")
        .expect("No runtime path set");

    let runtime_options: Vec<String> = matches
        .get_many::<String>("runtime-options")
        .with_context(|| "No OCI runtime options provided")?
        .cloned()
        .collect();

    let debug_output_dir = PathBuf::from(
        matches
            .get_one::<String>("debug-output-dir")
            .expect("No debug output dir set"),
    );

    let env_var_overrides: Vec<EnvVarOverride> = {
        let env_vars = matches
            .get_many::<EnvVar>("env-vars")
            .unwrap_or_default()
            .map(|e| EnvVarOverride::new(e, false));
        let env_vars_forced = matches
            .get_many::<EnvVar>("env-vars-forced")
            .unwrap_or_default()
            .map(|e| EnvVarOverride::new(e, true));
        env_vars.chain(env_vars_forced).collect()
    };

    // Intercept "create" commands to the underlying OCI runtime
    //
    // As a heuristic, we look for the -b or --bundle flag in the provided options. This is not
    // defined in the spec, but is used by runc for its "create" and "run" commands and appears to
    // have been adopted by most(?) other runtimes for compatibility purposes.
    if let Some(bundle_path) = get_bundle_path(&mut runtime_options.clone()) {
        // Load initial OCI config
        let config_path = bundle_path.join("config.json");
        let mut spec_modified = false;
        let mut spec = Spec::load(&config_path)
            .with_context(|| "Unable to parse OCI runtime specification")?;
        if matches.get_flag("write-debug-output") {
            fs::create_dir_all(&debug_output_dir)?;
            let hostname = spec
                .hostname()
                .clone()
                .unwrap_or(String::from("unknown_hostname"));
            let original_config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path)?)?;
            let original_filename = hostname.clone() + "_original.json";
            let original_file = fs::File::create(debug_output_dir.join(original_filename))?;
            serde_json::to_writer_pretty(&original_file, &original_config)?;

            let parsed_filename = hostname + "_parsed.json";
            let parsed_file = fs::File::create(debug_output_dir.join(parsed_filename))?;
            serde_json::to_writer_pretty(&parsed_file, &spec)?;
        }

        // Make any enabled modifications.
        //
        // Networking mounts are always made read-only. Docker bind-mounts these three
        // files from outside the container's writable layer, so where XFS project quotas
        // restrict that layer they are an escape hatch for filling the host volume -- see
        // "Read-only networking mounts" in the README. On a shared host that affects every
        // other container, which is what makes it worth enforcing unconditionally.
        //
        // Note this is not a barrier between containers: each gets its own copy of these
        // files, so tampering only ever affects the container doing it, and a process with
        // code execution can bypass them anyway (HOSTALIASES, RES_OPTIONS, LD_PRELOAD, or
        // simply querying a resolver directly). Treat it as quota enforcement, with
        // in-container tamper resistance as a side effect.
        spec_modified |= modify_networking_mounts(&mut spec);
        if !env_var_overrides.is_empty() {
            modify_env_vars(&mut spec, env_var_overrides);
            spec_modified = true;
        }

        // Write the updated config back out to disk
        if spec_modified {
            if matches.get_flag("write-debug-output") {
                let output_filename = spec
                    .hostname()
                    .clone()
                    .unwrap_or(String::from("unknown_hostname"))
                    + "_modified.json";
                fs::create_dir_all(&debug_output_dir)?;
                let modified_spec = fs::File::create(debug_output_dir.join(output_filename))?;
                serde_json::to_writer_pretty(&modified_spec, &spec)?;
            }
            spec.save(&config_path)
                .with_context(|| "Unable to write updated OCI runtime specification")?;
        }
    }

    // Forward call to the underlying runtime
    if matches.get_flag("write-debug-output") {
        fs::create_dir_all(&debug_output_dir)?;
        let runtime_calls = std::fs::File::options()
            .create(true)
            .append(true)
            .open(debug_output_dir.join("runtime_calls.log"))?;
        let mut runtime_calls = std::io::BufWriter::new(runtime_calls);
        runtime_calls
            .write_all(format!("{} {}\n", runtime_path, runtime_options.join(" ")).as_bytes())?;
        runtime_calls.flush()?;
    }
    // On success this replaces the current process, so nothing below runs.
    Err(call_oci_runtime(runtime_path, runtime_options))
}

/// Extracts the container bundle path from the trailing runtime options, if present.
///
/// clap cannot handle parsing this because we don't know that --bundle will appear first in the
/// list of options to forward to the runtime, and only trailing varargs can be captured.
fn get_bundle_path(options: &mut [String]) -> Option<PathBuf> {
    let mut options = options.iter();
    if let Some(bundle_opt) = options.find(|s| s.starts_with("-b") || s.starts_with("--bundle")) {
        return match bundle_opt.split_once('=') {
            Some((_option, path)) => Some(PathBuf::from(path)),
            None => options.next().map(PathBuf::from),
        };
    }
    None
}

/// Replaces this process with the actual OCI runtime, passing along any runtime options.
///
/// This execs rather than spawning a child and waiting on it, so that the runtime inherits
/// our PID and we leave the process tree entirely. A wrapper that spawns and waits is
/// wrong for an OCI runtime in two ways:
///
/// - containerd's shim invokes the runtime through go-runc, which builds commands with
///   Go's `exec.CommandContext`. On a cancelled or timed-out call that delivers SIGKILL to
///   the direct child only. With a wrapper in between, the wrapper dies and the real
///   runtime is orphaned, while the shim believes the call was cancelled. For a
///   `runc delete` that leaves the task undeleted and its shim never shut down.
/// - A runtime killed by a signal was reported to the caller as exit code -1, which
///   `process::exit` truncates to 255. Callers that distinguish signal death from a 255
///   exit status saw the wrong thing. Exec'ing reports the real wait status.
///
/// Returns only on failure to exec.
fn call_oci_runtime(runtime_path: &str, options: Vec<String>) -> anyhow::Error {
    let err = process::Command::new(runtime_path)
        .args(options.as_slice())
        .exec();
    anyhow::Error::new(err).context(format!(
        "Failed to execute underlying OCI runtime: {runtime_path}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn finds_bundle_short_flag_space_separated() {
        let mut o = opts(&["create", "-b", "/tmp/bundle", "cid"]);
        assert_eq!(get_bundle_path(&mut o), Some(PathBuf::from("/tmp/bundle")));
    }

    #[test]
    fn finds_bundle_short_flag_with_equals() {
        let mut o = opts(&["create", "-b=/tmp/bundle", "cid"]);
        assert_eq!(get_bundle_path(&mut o), Some(PathBuf::from("/tmp/bundle")));
    }

    #[test]
    fn finds_bundle_long_flag_space_separated() {
        let mut o = opts(&["create", "--bundle", "/tmp/bundle", "cid"]);
        assert_eq!(get_bundle_path(&mut o), Some(PathBuf::from("/tmp/bundle")));
    }

    #[test]
    fn finds_bundle_long_flag_with_equals() {
        let mut o = opts(&["create", "--bundle=/tmp/bundle", "cid"]);
        assert_eq!(get_bundle_path(&mut o), Some(PathBuf::from("/tmp/bundle")));
    }

    #[test]
    fn returns_none_when_no_bundle_flag_present() {
        let mut o = opts(&["start", "cid"]);
        assert_eq!(get_bundle_path(&mut o), None);
    }

    #[test]
    fn returns_none_for_empty_options() {
        let mut o: Vec<String> = Vec::new();
        assert_eq!(get_bundle_path(&mut o), None);
    }

    #[test]
    fn returns_none_when_short_flag_has_no_following_arg() {
        let mut o = opts(&["create", "-b"]);
        assert_eq!(get_bundle_path(&mut o), None);
    }
}
