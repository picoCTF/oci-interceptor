use anyhow::{Context, Result};
use clap::{crate_authors, crate_description, crate_name, crate_version, App, AppSettings, Arg};
use oci_spec::runtime::Spec;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<()> {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .setting(AppSettings::TrailingVarArg)
        .setting(AppSettings::DontDelimitTrailingValues)
        .setting(AppSettings::AllowHyphenValues)
        .arg(
            Arg::new("runtime-path")
                .long("--runtime-path")
                .default_value("runc")
                .help("Path to OCI runtime."),
        )
        .arg(
            Arg::new("readonly-networking-mounts")
                .long("--readonly-networking-mounts")
                .takes_value(false)
                .help("Whether to mount networking files as readonly."),
        )
        .arg(
            Arg::new("runtime-options")
                .multiple_occurrences(true)
                .allow_hyphen_values(true)
                .help("All additional options will be forwarded to the OCI runtime."),
        )
        .get_matches();

    let runtime_options = matches
        .values_of("runtime-options")
        .with_context(|| "No OCI runtime options provided")?;

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

        if matches.is_present("readonly-networking-mounts") {
            modify_network_mounts(&config_path)?;
        }
    }

    std::process::exit(call_oci_runtime(
        matches.value_of("runtime-path").unwrap(),
        runtime_options.collect(),
    )?);
}

/// Extracts the container bundle path from the trailing runtime options, if present.
///
/// clap cannot handle parsing this because we don't know that --bundle will appear first in the
/// list of options to forward to the runtime, and only trailing varargs can be captured.
fn get_bundle_path(options: &mut clap::Values) -> Option<PathBuf> {
    if let Some(bundle_opt) = options.find(|s| s.starts_with("-b") || s.starts_with("--bundle")) {
        return match bundle_opt.split_once('=') {
            Some((_option, path)) => Some(PathBuf::from(path)),
            None => options.next().map(PathBuf::from),
        };
    }
    None
}

/// Calls the actual OCI runtime, passing along any runtime options.
fn call_oci_runtime(runtime_path: &str, options: Vec<&str>) -> Result<i32> {
    let mut child = Command::new(runtime_path)
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
