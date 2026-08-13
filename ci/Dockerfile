# CI image for solar-monitor — Rust + Node + aarch64 cross toolchain.
#
# Build it on the runner host (not in a workflow):
#   docker build -t solar-monitor-ci:latest ci/
#
# node in the image is not optional: the runner execs every JavaScript action
# (actions/checkout included) inside the job container, so an image without
# node fails before the first step runs.
#
# Rebuild when a pinned version below changes. Everything else (cargo
# registry, target dirs, npm cache) lives in named volumes mounted by the
# workflow, so day-to-day runs never touch this image.
FROM docker.io/library/rust:1.96-bookworm

ENV DEBIAN_FRONTEND=noninteractive

# gcc-aarch64-linux-gnu: the Pi cross-build's linker (named in
# .cargo/config.toml) and the C compiler cc-rs picks for the bundled SQLite in
# libsqlite3-sys. libc6-dev-arm64-cross supplies the matching headers/libc.
# file + binutils (for `strings`): the pi job's "is this really an aarch64 ELF
# with assets embedded" assertions.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl gnupg git jq \
        gcc-aarch64-linux-gnu libc6-dev-arm64-cross file binutils \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Node 22 bundles npm 10; web/package-lock.json is authored by npm 11 on the
# dev box. CI must run the same npm that writes the lock, or `npm ci` can
# reject a lockfile that is actually fine.
RUN npm install -g npm@11.14.1

RUN rustup component add rustfmt clippy \
    && rustup target add aarch64-unknown-linux-gnu

# For the `audit` job. Deliberately unpinned: cargo-audit ships the advisory
# database client, and holding it back is how you end up not being told about
# something.
RUN cargo install cargo-audit --locked && rm -rf /usr/local/cargo/registry/*

# Workflows bind their cache volumes over these.
ENV CARGO_TARGET_DIR=/build-target \
    npm_config_cache=/npm-cache

RUN mkdir -p /build-target /npm-cache
