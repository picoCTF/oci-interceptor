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

Currently, prebuilt binaries are only available for x86 Linux (glibc-based). Other platforms must installed from source.

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
          Mount networking files as readonly
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
            "path": "/usr/local/bin/oci-interceptor",
            "runtimeArgs": [
                "--oi-readonly-networking-mounts"
            ]
        }
    }
}
```
The Docker daemon must be restarted (`systemctl restart docker.service`) in order to apply changes to this configuration file.

Note that if you set `oci-interceptor` as the default runtime, you can still bypass it for a specific container by specifying `docker run --runtime=runc`.

While it is not possible to override `runtimeArgs` with a `docker run` option, you could specify multiple interceptor "runtimes" (with different flags) and switch between them using `docker run --runtime=<name>`.

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

To avoid this issue, specify the `--oi-readonly-networking-mounts` flag. This modifies these mounts to be read-only, preventing writes from inside the container.

#### Related issues

- Workaround for [moby#13152](https://github.com/moby/moby/issues/41991), [moby#41991](https://github.com/moby/moby/issues/41991) (without custom bind mounts or making entire rootfs readonly)
- Optionally reverts [moby#5129](https://github.com/moby/moby/pull/5129)

### Overriding environment variables

Allows specifying default environment variable values for containers without using `docker run --env` or `--env-file`.

Use `--oi-env <NAME=VALUE>` to set a default for an environment variable. This will not take precedence over a value explicitly specified via `docker run --env` or `--env-file`.

Alternatively, use `--oi-env-force <NAME=VALUE>` to force an certain value even when otherwise specified via `docker run --env` or `--env-file`.

#### Related issues
- Workaround for [moby#16699](https://github.com/moby/moby/issues/16699) (supports arbitrary environment variables, not only proxy config)
- Solution for https://stackoverflow.com/questions/33775075/how-to-set-default-docker-environment-variables
- Solution for https://stackoverflow.com/questions/50644143/dockerd-set-default-environment-variable-for-all-containers

### Debug output

Specify the `--oi-write-debug-output` flag to write original, parsed, and modified container configs to the directory specified as `--oi-debug-output-dir` (default `/var/log/oci-interceptor`).

The resulting files will be named:
- `<container_hostname>_original.json` (the original config)
- `<container_hostname>_parsed.json` (the parsed config)
- `<container_hostname>_modified.json` (the modified config, only written if modification occurred)

Additionally, forwarded calls to the underlying OCI runtime will be appended to the file `runtime_calls.txt` within the debug output directory.
