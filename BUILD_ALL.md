# Build all

`loom-bootstrap/scripts/build-all.sh` builds every existing Cargo workspace in
the suite. It uses release mode by default and fails if a declared workspace is
missing or a build command fails.

```bash
cd loom-bootstrap
bash scripts/env-check.sh
bash scripts/build-all.sh --release
```

The eight application binaries are:

```text
loom-writer/target/release/loom-writer
loom-sheets/target/release/loom-sheets
loom-present/target/release/loom-present
loom-photo/target/release/loom-photo
loom-motion/target/release/loom-motion
loom-video/target/release/loom-video
loom-studio/target/release/loom-studio
loom-encode/target/release/loom-encode
```

The remaining Cargo workspaces are `loom-core`, `loom-vision`, and
`loom-plugin-sdk`; they do not provide an application binary expected by the
smoke runner. The build gate currently covers all 11 workspaces.

For Docker image construction and CI checks:

```bash
bash scripts/docker-build.sh
bash scripts/docker-test.sh
```

Docker visual QA uses the Ubuntu software-rendered Xvfb image. The visual image
inherits all required packages from the CI image and does not install optional
packages at runtime.
