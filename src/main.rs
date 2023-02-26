use anyhow::{Context, Result};
use clap::{crate_authors, crate_description, crate_name, crate_version, Arg, ArgAction};
use oci_spec::runtime::Spec;
use std::{
    path::{Path, PathBuf},
    process,
};

fn main() -> Result<()> {
    let matches = clap::Command::new(crate_name!())
        .version(crate_version!())
        .disable_version_flag(true)
        .disable_help_flag(true)
        .author(crate_authors!())
        .about(crate_description!())
        .trailing_var_arg(true)
        .dont_delimit_trailing_values(true)
        .allow_hyphen_values(true)
        .arg(
            Arg::new("runtime-path")
                .long("oi-runtime-path")
                .default_value("runc")
                .help("Path to OCI runtime."),
        )
        .arg(
            Arg::new("readonly-networking-mounts")
                .long("oi-readonly-networking-mounts")
                .action(ArgAction::SetTrue)
                .help("Whether to mount networking files as readonly."),
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
                .allow_hyphen_values(true)
                .help("All additional options will be forwarded to the OCI runtime."),
        )
        .get_matches();

    let runtime_options: Vec<String> = matches
        .get_many::<String>("runtime-options")
        .with_context(|| "No OCI runtime options provided")?
        .cloned()
        .collect();

    // We only want to intercept the runtime's "create" command (per the OCI runtime spec).
    //
    // As a heuristic, we look for the -b or --bundle flag in the provided options. This is not
    // defined in the spec, but is used by runc for its "create" and "run" commands and appears to
    // have been adopted by most(?) other runtimes for compatibility purposes.
    //
    // TODO: The runc "spec" command will probably cause an error if called with the -b or --bundle
    // flag, since oci-interceptor will attempt to modify a config.json that does not yet exist.
    // However, in practice this should usually not be a problem as Docker doesn't call this
    // command. Eventually, we may want to do more involved introspection of the runtime options to
    // decide whether container creation is occurring, though this may be difficult to do in a
    // runtime-agnostic way since the spec does not enforce any specific CLI command design.
    if let Some(bundle_path) = get_bundle_path(&mut runtime_options.clone()) {
        let config_path = bundle_path.join("config.json");

        if matches.get_flag("readonly-networking-mounts") {
            modify_network_mounts(&config_path)?;
        }
    }

    std::process::exit(call_oci_runtime(
        matches.get_one::<String>("runtime-path").unwrap(),
        runtime_options,
    )?);
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

/// Calls the actual OCI runtime, passing along any runtime options.
fn call_oci_runtime(runtime_path: &str, options: Vec<String>) -> Result<i32> {
    let mut child = process::Command::new(runtime_path)
        .args(options.as_slice())
        .spawn()
        .with_context(|| "Failed to execute underlying OCI runtime")?;
    let status = child
        .wait()
        .with_context(|| "Failed to wait on OCI runtime process")?;
    match status.code() {
        Some(code) => Ok(code),
        None => Ok(-1), // child process was killed by a signal
    }
}

/// Modifies the mounts for these networking-related files:
///
/// - /etc/hosts
/// - /etc/hostname
/// - /etc/resolv.conf
///
/// in the container config, making them read-only.
fn modify_network_mounts(config_path: &Path) -> Result<()> {
    let mut spec =
        Spec::load(config_path).with_context(|| "Unable to parse OCI runtime specification")?;
    if let Some(mounts) = spec.mounts() {
        let mut mounts = mounts.clone();
        for mount in mounts.iter_mut() {
            match mount.destination().to_str() {
                Some("/etc/hosts") | Some("/etc/hostname") | Some("/etc/resolv.conf") => {
                    if let Some(options) = mount.options() {
                        if !options.contains(&"ro".into()) {
                            let mut options = options.clone();
                            options.push("ro".into());
                            mount.set_options(Some(options));
                        }
                    }
                }
                Some(_) => {}
                None => {}
            }
        }
        spec.set_mounts(Some(mounts));
        spec.save(config_path)
            .with_context(|| "Unable to write updated OCI runtime specification")?;
    }
    Ok(())
}
