mod networking_mounts;

use anyhow::{Context, Result};
use clap::{crate_authors, crate_description, crate_name, crate_version, Arg, ArgAction};
use networking_mounts::modify_networking_mounts;
use oci_spec::runtime::Spec;
use std::{fs, io::Write, path::PathBuf, process};

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
                .default_value("runc")
                .help("Path to OCI runtime."),
        )
        .arg(
            Arg::new("readonly-networking-mounts")
                .long("oi-readonly-networking-mounts")
                .action(ArgAction::SetTrue)
                .help("Mount networking files as readonly."),
        )
        .arg(
            Arg::new("write-debug-output")
                .long("oi-write-debug-output")
                .action(ArgAction::SetTrue)
                .help("Write debug output to --oi-debug-output-dir."),
        )
        .arg(
            Arg::new("debug-output-dir")
                .long("oi-debug-output-dir")
                .default_value("/var/log/oci-interceptor")
                .help("Debug output location when --oi-write-debug-output is enabled."),
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

        // Make any enabled modifications
        if matches.get_flag("readonly-networking-mounts") {
            modify_networking_mounts(&mut spec);
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
            .open(debug_output_dir.join("runtime_calls.txt"))?;
        let mut runtime_calls = std::io::BufWriter::new(runtime_calls);
        runtime_calls
            .write_all(format!("{} {}\n", runtime_path, runtime_options.join(" ")).as_bytes())?;
        runtime_calls.flush()?;
    }
    std::process::exit(call_oci_runtime(runtime_path, runtime_options)?);
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
