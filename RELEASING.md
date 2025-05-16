# Releasing

1. Ensure that [`CHANGELOG.md`](./CHANGELOG.md) is up-to-date with all relevant changes. Additionally, make sure that the version specified in [Cargo.toml](./Cargo.toml) matches the to-be-released version.

1. Create and push a new Git tag for the version with a `v` prefix, e.g.:
    ```shell
    git tag vX.Y.Z
    git push --tags
    ```
    This will automatically run the "Publish release" GitHub Actions workflow, which will create a GitHub release, build a release tarball, and attach the tarball to the release.

1. Edit the newly created GitHub release, setting the relevant section of `CHANGELOG.md` as the description.
