# Changelog

## v0.2.3

- Switched CI from `dtolnay/rust-toolchain` to `actions-rust-lang/setup-rust-toolchain`. CI builds now compile with `RUSTFLAGS=-D warnings` and surface rustc/rustfmt warnings as PR annotations; release builds are exempt so a new stable lint cannot break a tagged release. Dependabot now tracks GitHub Actions pins.
- Documented the glibc floor of the prebuilt `x86_64-unknown-linux-gnu` binary: it is built on `ubuntu-24.04` runners (matching the challenge server target) and requires glibc 2.39 or newer (Ubuntu 24.04, Debian 13, RHEL 10, Fedora 40, or newer). The v0.2.2 binary already had this floor; no runners or artifacts changed. The release workflow now checks that the built binary's target matches the tarball name.

## v0.2.2

- Fixed an issue where `--oi-env` overrides were silently discarded unless `--oi-readonly-networking-mounts` was also passed.
- Changed the file extension of `--oi-write-debug-output` runtime call dumps from `.json` to `.log`.
- Upgraded the crate to the Rust 2024 edition.
- Relicensed under dual MIT OR Apache-2.0.
- Added `RELEASING.md` documenting the release process, plus CI test coverage (unit tests, CLI smoke tests, tests-in-CI).
- Bumped dependencies to current versions (`clap`, `anyhow`, `serde_json`, `oci-spec`).

## v0.2.1

- Reverted to upstream OCI spec parsing library.

## v0.2.0

- All options are now prefixed with `--oi` in order to avoid name conflicts with underlying runtime options. For example, `--readonly-networking-mounts` is now called `--oi-readonly-networking-mounts`.
- Fixed an issue where rewriting a container's config resulted in `clone3` syscalls failing. This was due to an issue in the OCI spec parsing dependency. This release uses a forked version of the library, pending acceptance of an upstream PR to resolve the issue.
- Added the ability to override environment variables (`--oi-env`, `--oi-env-force`).
- Added optional debug output when modifying container configs (`--oi-write-debug-output`).

## v0.1.0

Initial release. The `--readonly-networking-mounts` flag is supported, which causes `/etc/hosts`, `/etc/hostname`, and `/etc/resolv.conf` to be mounted as readonly. Typically, Docker will mount these files as read-write, which can be problematic for containers with a writable layer size quota.
