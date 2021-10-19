# Changelog

## v0.1.0

Initial release. The `--readonly-networking-mounts` flag is supported, which causes `/etc/hosts`, `/etc/hostname`, and `/etc/resolv.conf` to be mounted as readonly. Typically, Docker will mount these files as read-write, which can be problematic for containers with a writable layer size quota.
