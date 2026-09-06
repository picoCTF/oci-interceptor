use oci_spec::runtime::Spec;

/// Modifies the mounts for these networking-related files:
///
/// - /etc/hosts
/// - /etc/hostname
/// - /etc/resolv.conf
///
/// in the container config, making them read-only.
///
/// The mounts themselves are left in place. Docker points a container on a user-defined
/// network at its embedded DNS resolver by writing `nameserver 127.0.0.11` into the
/// bind-mounted /etc/resolv.conf, so removing that mount would break resolution of
/// sibling containers by name. Only write access is taken away.
///
/// Returns whether any mount was actually changed.
pub(crate) fn modify_networking_mounts(spec: &mut Spec) -> bool {
    let Some(mounts) = spec.mounts() else {
        return false;
    };
    let mut mounts = mounts.clone();
    let mut changed = false;
    for mount in mounts.iter_mut() {
        match mount.destination().to_str() {
            Some("/etc/hosts") | Some("/etc/hostname") | Some("/etc/resolv.conf") => {
                // Absent options are still writable, and the kernel takes the last of a
                // conflicting pair, so an existing "ro" earlier in the list does not
                // survive a later "rw". Drop both and re-append "ro" so it always wins.
                let current = mount.options().clone().unwrap_or_default();
                let mut wanted: Vec<String> = current
                    .iter()
                    .filter(|option| *option != "ro" && *option != "rw")
                    .cloned()
                    .collect();
                wanted.push(String::from("ro"));
                if wanted != current {
                    mount.set_options(Some(wanted));
                    changed = true;
                }
            }
            _ => {}
        }
    }
    if changed {
        spec.set_mounts(Some(mounts));
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::modify_networking_mounts;
    use oci_spec::runtime::Spec;

    fn spec_with(mounts: &str) -> Spec {
        serde_json::from_str(&format!("{{\"ociVersion\":\"1.0.0\",\"mounts\":{mounts}}}"))
            .expect("test spec should parse")
    }

    fn options_of(spec: &Spec, destination: &str) -> Vec<String> {
        spec.mounts()
            .as_ref()
            .expect("spec should have mounts")
            .iter()
            .find(|mount| mount.destination().to_str() == Some(destination))
            .expect("mount should be present")
            .options()
            .clone()
            .unwrap_or_default()
    }

    #[test]
    fn a_mount_without_options_still_becomes_readonly() {
        let mut spec = spec_with("[{\"destination\":\"/etc/hosts\"}]");
        assert!(modify_networking_mounts(&mut spec));
        assert_eq!(options_of(&spec, "/etc/hosts"), vec!["ro"]);
    }

    #[test]
    fn a_writable_mount_keeps_its_other_options() {
        let mut spec =
            spec_with("[{\"destination\":\"/etc/hosts\",\"options\":[\"rbind\",\"rw\"]}]");
        assert!(modify_networking_mounts(&mut spec));
        assert_eq!(options_of(&spec, "/etc/hosts"), vec!["rbind", "ro"]);
    }

    #[test]
    fn a_later_rw_does_not_survive_an_earlier_ro() {
        // The kernel takes the last of a conflicting pair, so this mount was writable.
        let mut spec =
            spec_with("[{\"destination\":\"/etc/resolv.conf\",\"options\":[\"ro\",\"rw\"]}]");
        assert!(modify_networking_mounts(&mut spec));
        assert_eq!(options_of(&spec, "/etc/resolv.conf"), vec!["ro"]);
    }

    #[test]
    fn an_already_readonly_mount_reports_no_change() {
        let mut spec =
            spec_with("[{\"destination\":\"/etc/hostname\",\"options\":[\"rbind\",\"ro\"]}]");
        assert!(!modify_networking_mounts(&mut spec));
        assert_eq!(options_of(&spec, "/etc/hostname"), vec!["rbind", "ro"]);
    }

    #[test]
    fn unrelated_mounts_are_left_writable() {
        let mut spec = spec_with("[{\"destination\":\"/tmp\",\"options\":[\"rw\"]}]");
        assert!(!modify_networking_mounts(&mut spec));
        assert_eq!(options_of(&spec, "/tmp"), vec!["rw"]);
    }
}
