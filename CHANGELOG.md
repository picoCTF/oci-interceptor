# Changelog

## v0.2.1

- Reverted to upstream OCI spec parsing library.

## v0.2.0

- All options are now prefixed with `--oi` in order to avoid name conflicts with underlying runtime options. For example, `--readonly-networking-mounts` is now called `--oi-readonly-networking-mounts`.
- Fixed an issue where rewriting a container's config resulted in `clone3` syscalls failing. This was due to an issue in the OCI spec parsing dependency. This release uses a forked version of the library, pending acceptance of an upstream PR to resolve the issue.
- Added the ability to override environment variables (`--oi-env`, `--oi-env-force`).
- Added optional debug output when modifying container configs (`--oi-write-debug-output`).

## v0.1.0

Initial release. The `--readonly-networking-mounts` flag is supported, which causes `/etc/hosts`, `/etc/hostname`, and `/etc/resolv.conf` to be mounted as readonly. Typically, Docker will mount these files as read-write, which can be problematic for containers with a writable layer size quota.
