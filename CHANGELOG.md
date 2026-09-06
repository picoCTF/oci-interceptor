# Changelog

## v0.3.0

- The OCI runtime is now `exec`'d rather than spawned as a child process. containerd's shim invokes the runtime through go-runc, which builds commands with Go's `exec.CommandContext`; a cancelled or timed-out call delivers SIGKILL to the direct child only, so the interceptor died and the real runtime was orphaned beneath it. For a `runc delete` that left the task undeleted and its `containerd-shim-runc-v2` resident indefinitely, at roughly 5 MiB each until the host was rebooted.
- A runtime killed by a signal is now reported as signal death. Previously it was mapped to exit code -1, which `process::exit` truncates to 255 — indistinguishable from a runtime that genuinely exited 255.
- **Deployment change required for the above to take effect.** Docker generates its own non-`exec` `/bin/sh` wrapper for any runtime whose `runtimeArgs` is non-empty, which re-inserts exactly the process layer this release removes. Pass the interceptor's flags from a wrapper script of your own that `exec`s, name that script as the runtime's `path`, and leave `runtimeArgs` unset. See "Passing flags" in the README.
- Networking mounts (`/etc/hosts`, `/etc/hostname`, `/etc/resolv.conf`) are now always made read-only. This is a behaviour change for anyone who did not previously pass `--oi-readonly-networking-mounts`, and there is no opt-out. The flag is retained as an accepted no-op so an existing `daemon.json` that passes it keeps working. The mounts themselves are left in place, so Docker's embedded DNS resolver still works for sibling-container name resolution. Mount options are normalised rather than appended to: a mount that arrives with no options at all, or with a `rw` following a `ro`, now ends up read-only too, because `ro` is always placed last and that is the flag the kernel honours.
- The container spec is only written back to disk when a modification actually changed it.

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
