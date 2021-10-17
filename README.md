# OCI Interceptor

An OCI runtime wrapper that modifies containers'
[`config.json`](https://github.com/opencontainers/runtime-spec/blob/master/spec.md) according to
specified rules before forwarding the container to a real OCI runtime for creation.

This can be used to enforce certain policies on created containers, or to work around limitations
in higher-level tools like Docker.

Note that programs like Docker track their own container metadata, which may not accurately reflect
these last-minute changes to the underlying OCI configuration.

## Usage

```bash
$ oci-interceptor \
  [--runtime-path=runc] \
  [--readonly-networking-mounts] \
  [...other flags forwarded to runtime]
```

### With Docker

The [Docker daemon
configuration](https://docs.docker.com/engine/reference/commandline/dockerd/#daemon-configuration-file)
must be modified to add this runtime. If you want it to be invoked every time a container is
created, you should also make it the default runtime (instead of `runc`).

If you are not using a custom OCI runtime like `crun` or `youki`, you can omit the `--runtime-path`
option, as it defaults to `runc`, the default runtime included with Docker.

#### Example `/etc/docker/daemon.json` contents

```json
{
    "default-runtime": "oci-interceptor",
    "runtimes": {
        "oci-interceptor": {
            "path": "/usr/local/bin/oci-interceptor",
            "runtimeArgs": [
                "--runtime-path=runc",
                "--readonly-networking-mounts"
            ]
        }
    }
}
```

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

To avoid this issue, specify the `--readonly-networking-mounts` flag, which automatically modifies
these mounts to be read-only, preventing writes from inside the container.

### Arbitrary modifications

The intent of this project is to eventually support arbitrary modifications to `config.json` via a
list of specified rules, but work on this has not yet begun.
