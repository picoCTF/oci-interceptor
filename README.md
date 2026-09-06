# OCI Interceptor

An OCI runtime wrapper that modifies containers'
[runtime configuration](https://github.com/opencontainers/runtime-spec/blob/main/config.md) according to
specified rules before forwarding the container to a real runtime for creation.

This can be used to enforce certain policies on created containers, or to work around limitations
in higher-level container management tools such as Docker.

## Installation

Download the latest [release](https://github.com/picoCTF/oci-interceptor/releases), extract the tarball, and copy the binary to an appropriate location:

```bash
$ tar xzf oci-interceptor_x86_64-unknown-linux-gnu.tar.gz
$ cp oci-interceptor /usr/local/bin
```

Alternatively, build and install from source:

```bash
$ cargo install --locked --path .
```

Currently, prebuilt binaries are only available for x86-64 Linux with glibc 2.39 or newer (e.g. Ubuntu 24.04). Other platforms must be installed from source.

## Usage

All `oci-interceptor` flags are prefixed with `--oi` in order to avoid conflicts with the underlying OCI runtime.

```
Usage: oci-interceptor [OPTIONS] [runtime-options]...

Arguments:
  [runtime-options]...  All additional options will be forwarded to the OCI runtime.

Options:
      --oi-runtime-path <runtime-path>
          Path to OCI runtime. [default: runc]
      --oi-readonly-networking-mounts
          Deprecated, and now a no-op: networking mounts are always readonly
      --oi-write-debug-output
          Write debug output
      --oi-debug-output-dir <debug-output-dir>
          Debug output location [default: /var/log/oci-interceptor]
      --oi-env <NAME=VALUE>
          Set an environment variable if not already present in config
      --oi-env-force <NAME=VALUE>
          Override an environment variable, regardless of any original value
      --oi-version
          Print version
      --oi-help
          Print help
```

### With Docker

The [Docker daemon
configuration](https://docs.docker.com/engine/reference/commandline/dockerd/#daemon-configuration-file)
must be modified to add this runtime. If you want it to be invoked every time a container is
created, you should also make it the default runtime (instead of `runc`).

If you are not using an alternative OCI runtime such as [`crun`](https://github.com/containers/crun) or [`youki`](https://github.com/containers/youki), you can omit the `--oi-runtime-path`
option, as it defaults to `runc`, the default runtime bundled with Docker.

#### Example `/etc/docker/daemon.json` contents

```json
{
    "default-runtime": "oci-interceptor",
    "runtimes": {
        "oci-interceptor": {
            "path": "/usr/local/bin/oci-interceptor"
        }
    }
}
```
The Docker daemon must be restarted (`systemctl restart docker.service`) in order to apply changes to this configuration file.

Note that if you set `oci-interceptor` as the default runtime, you can still bypass it for a specific container by specifying `docker run --runtime=runc`.

#### Passing flags: use a wrapper script, not `runtimeArgs`

**Do not put the interceptor's flags in `runtimeArgs`.** Docker generates its own wrapper
script for any runtime whose `runtimeArgs` is non-empty, and that generated wrapper does
not `exec`:

```sh
#!/bin/sh
/usr/local/bin/oci-interceptor --flags $@
```

It therefore stays in the process tree between the containerd shim and the real runtime.
containerd invokes the runtime through go-runc, which builds commands with Go's
`exec.CommandContext`; a cancelled or timed-out call delivers SIGKILL to the direct child
only — that shell — orphaning `runc` beneath it. For a `runc delete` the task is then never
deleted and its `containerd-shim-runc-v2` never shuts down, leaking roughly 5 MiB per
occurrence until the host is rebooted. That defeats the point of the interceptor exec'ing
the runtime at all.

If you have no flags to pass, you need no wrapper at all: name the binary itself as `path`,
as in the example above. Otherwise put the flags in a wrapper script of your own that
`exec`s, and name that script as `path`:

```sh
#!/bin/sh
exec /usr/local/bin/oci-interceptor --oi-env HTTP_PROXY=http://proxy.example:3128 "$@"
```

Keep `"$@"` quoted — Docker's generated wrapper uses a bare `$@`, which word-splits and
glob-expands its arguments. An absent or empty `runtimeArgs` generates no wrapper.

You can still run several interceptor "runtimes" with different flags and switch between
them using `docker run --runtime=<name>`; each one needs its own wrapper script, named by
its own `path`.

## Supported Customizations

### Read-only networking mounts

Works around the fact that Docker mounts the following files as read/write by default:

- `/etc/hosts`
- `/etc/hostname`
- `/etc/resolv.conf`

When XFS project quotas are used to [restrict a container's writable layer
size](https://github.com/moby/moby/pull/24771), these files provide an escape hatch for malicious
users to fill the host storage volume.

This can usually only be circumvented by manually creating read-only bind mounts over these paths (in which case Docker can no longer manage the container's DNS configuration) or by making the entire rootfs read-only (which severely constrains the workloads possible inside the container).

These mounts are always modified to be read-only, preventing writes from inside the container. The mounts themselves are left in place: Docker points a container on a user-defined network at its embedded DNS resolver by writing `nameserver 127.0.0.11` into the bind-mounted `/etc/resolv.conf`, so removing the mount would break resolution of sibling containers by name. Only write access is taken away.

The `--oi-readonly-networking-mounts` flag is retained as an accepted no-op so that an existing `daemon.json` passing it keeps working; it no longer has any effect.

#### Related issues

- Workaround for [moby#13152](https://github.com/moby/moby/issues/41991), [moby#41991](https://github.com/moby/moby/issues/41991) (without custom bind mounts or making entire rootfs readonly)
- Reverts [moby#5129](https://github.com/moby/moby/pull/5129)

### Overriding environment variables

Allows specifying default environment variable values for containers without using `docker run --env` or `--env-file`.

Use `--oi-env <NAME=VALUE>` to set a default for an environment variable. This will not take precedence over a value explicitly specified via `docker run --env` or `--env-file`.

Alternatively, use `--oi-env-force <NAME=VALUE>` to force an certain value even when otherwise specified via `docker run --env` or `--env-file`.

#### Related issues
- Workaround for [moby#16699](https://github.com/moby/moby/issues/16699) (supports arbitrary environment variables, not only proxy config)
- Solution for https://stackoverflow.com/questions/33775075/how-to-set-default-docker-environment-variables
- Solution for https://stackoverflow.com/questions/50644143/dockerd-set-default-environment-variable-for-all-containers

## Testing

Unit tests run with `cargo test`. End-to-end integration tests live in `tests/integration.rs` and exercise the wrapper through a real Docker daemon. They are gated by the `OCI_INTERCEPTOR_INTEGRATION` environment variable so the default `cargo test` invocation stays portable.

To run the integration tests locally on a Linux host with Docker:

1. Build and install the binary: `cargo build --release && sudo install -m 0755 target/release/oci-interceptor /usr/local/bin/oci-interceptor`
2. Configure `/etc/docker/daemon.json` with the named runtimes listed in the module docs of [tests/integration.rs](tests/integration.rs). The CI workflow (`.github/workflows/CI.yml`, `integration` job) shows the exact set the suite expects. Note that it is a test harness, not a reference deployment: it uses `runtimeArgs` because that is the only native per-runtime flag mechanism and the flag assertions need it. For a real host, follow [Passing flags](#passing-flags-use-a-wrapper-script-not-runtimeargs) instead.
3. `sudo systemctl restart docker`
4. `OCI_INTERCEPTOR_INTEGRATION=1 cargo test --test integration -- --test-threads=1`

CI runs the same suite on every push and pull request, which provides a regression check against the Docker and runc versions shipped on `ubuntu-latest` runners.

### Debug output

Specify the `--oi-write-debug-output` flag to write original, parsed, and modified container configs to the directory specified as `--oi-debug-output-dir` (default `/var/log/oci-interceptor`). As with every other flag, pass these from an exec'ing wrapper script rather than `runtimeArgs` — see [Passing flags](#passing-flags-use-a-wrapper-script-not-runtimeargs).

The resulting files will be named:
- `<container_hostname>_original.json` (the original config)
- `<container_hostname>_parsed.json` (the parsed config)
- `<container_hostname>_modified.json` (the modified config, only written if modification occurred)

Additionally, forwarded calls to the underlying OCI runtime will be appended to the file `runtime_calls.log` within the debug output directory.
