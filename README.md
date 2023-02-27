# OCI Interceptor

An OCI runtime wrapper that modifies containers'
[`config.json`](https://github.com/opencontainers/runtime-spec/blob/master/config.md) according to
specified rules before forwarding the container to a real OCI runtime for creation.

This can be used to enforce certain policies on created containers, or to work around limitations
in higher-level tools like Docker.

Note that programs like Docker track their own container metadata, which may not accurately reflect
these last-minute changes to the underlying OCI configuration.

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

All `oci-interceptor` flags are prefixed with `--oi-` in order to avoid conflicts with the underlying OCI runtime.

```bash
$ oci-interceptor \
  [--oi-runtime-path=runc] \
  [--oi-debug-output-dir=/var/log/oci-interceptor] \
  [--oi-write-debug-output] \
  [--oi-readonly-networking-mounts] \
  [...other flags forwarded to runtime]
```

### With Docker

The [Docker daemon
configuration](https://docs.docker.com/engine/reference/commandline/dockerd/#daemon-configuration-file)
must be modified to add this runtime. If you want it to be invoked every time a container is
created, you should also make it the default runtime (instead of `runc`).

If you are not using a custom OCI runtime like `crun` or `youki`, you can omit the `--oi-runtime-path`
option, as it defaults to `runc`, the default runtime included with Docker.

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

Even if you set `oci-interceptor` as the default runtime, you can still bypass it while running a container via `docker run --runtime=runc`.

While it is not possible to override `runtimeArgs` using a `docker run` option, you could specify several different "runtimes" (with different `runtimeArgs`) and switch between them using `docker run --runtime=<name>`.

## Supported Customizations

### Readonly networking mounts

Works around the fact that Docker mounts the following files as read/write by default (https://github.com/moby/moby/issues/41991):

- `/etc/hosts`
- `/etc/hostname`
- `/etc/resolv.conf`

When XFS project quotas are used to [restrict a container's writable layer
size](https://github.com/moby/moby/pull/24771), these files provide an escape hatch for malicious
users to fill the graph storage volume. This can be circumvented by manually mounting readonly files
over these paths, but in that case Docker can no longer manage the container's DNS configuration.

To avoid this issue, specify the `--oi-readonly-networking-mounts` flag, which automatically modifies
these mounts to be read-only, preventing writes from inside the container.

### Debug output

Specify the `--oi-write-debug-output` flag to write original, parsed, and modified container configs to the directory specified by `--oi-debug-output-dir` (default `/var/log/oci-interceptor`).

These filenames will have the format:
- `<container_hostname>_original.json` (the original config)
- `<container_hostname>_parsed.json` (the parsed config)
- `<container_hostname>_modified.json` (the modified config, only written if modification occurred)

Additionally, forwarded calls to the underlying OCI runtime will be appended to the file `runtime_calls.txt` within this directory.
