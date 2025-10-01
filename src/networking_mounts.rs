use oci_spec::runtime::Spec;

/// Modifies the mounts for these networking-related files:
///
/// - /etc/hosts
/// - /etc/hostname
/// - /etc/resolv.conf
///
/// in the container config, making them read-only.
pub(crate) fn modify_networking_mounts(spec: &mut Spec) {
    if let Some(mounts) = spec.mounts() {
        let mut mounts = mounts.clone();
        for mount in mounts.iter_mut() {
            match mount.destination().to_str() {
                Some("/etc/hosts") | Some("/etc/hostname") | Some("/etc/resolv.conf") => {
                    if let Some(options) = mount.options()
                        && !options.contains(&"ro".into())
                    {
                        let mut options = options.clone();
                        options.push("ro".into());
                        mount.set_options(Some(options));
                    }
                }
                Some(_) => {}
                None => {}
            }
        }
        spec.set_mounts(Some(mounts));
    }
}
