use anyhow::Context;
use oci_spec::runtime::Spec;

#[derive(Clone, Debug)]
pub(crate) struct EnvVar {
    name: String,
    value: String,
}

impl EnvVar {
    pub(crate) fn new(name: &str, value: &str) -> Self {
        Self {
            name: String::from(name),
            value: String::from(value),
        }
    }
}

pub(crate) fn parse_env_var(value: &str) -> Result<EnvVar, anyhow::Error> {
    let err_msg = "environment variables must be in NAME=VALUE format";
    let (name, value) = value.split_once('=').context(err_msg)?;
    if name.is_empty() || value.is_empty() {
        return Err(anyhow::anyhow!(err_msg));
    }
    Ok(EnvVar::new(name, value))
}

#[derive(Clone, Debug)]
pub(crate) struct EnvVarOverride {
    name: String,
    value: String,
    force: bool,
}

impl EnvVarOverride {
    pub(crate) fn new(var: &EnvVar, force: bool) -> Self {
        Self {
            name: var.name.clone(),
            value: var.value.clone(),
            force,
        }
    }
}

/// Overrides environment variables in the container config.
pub(crate) fn modify_env_vars(spec: &mut Spec, vars: Vec<EnvVarOverride>) {
    if let Some(process) = spec.process() {
        let mut new_process = process.clone();
        if let Some(env) = process.env() {
            let mut new_env = env.clone();
            for var in vars {
                let mut found = false;
                for existing_var in new_env.iter_mut() {
                    if existing_var.starts_with(&format!("{}=", var.name)) {
                        found = true;
                        if var.force {
                            *existing_var = format!("{}={}", var.name, var.value);
                        }
                        break;
                    }
                }
                if !found {
                    new_env.push(format!("{}={}", var.name, var.value));
                }
            }
            new_process.set_env(Some(new_env));
        }
        spec.set_process(Some(new_process));
    }
}
