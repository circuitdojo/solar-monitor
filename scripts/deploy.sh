#!/bin/bash

# Check for release flag
BUILD_TYPE="debug"
BUILD_FLAGS=""
if [[ "$1" == "--release" || "$1" == "-r" ]]; then
    BUILD_TYPE="release"
    BUILD_FLAGS="--release"
fi

export PKG_CONFIG_ALLOW_CROSS=1
export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc

echo "Building ${BUILD_TYPE} version..."
cargo build --target aarch64-unknown-linux-gnu $BUILD_FLAGS

echo "Copying binary to Pi..."
scp target/aarch64-unknown-linux-gnu/${BUILD_TYPE}/solar-monitor pi@solar-pi.local:~/
ssh pi@solar-pi.local 'chmod +x ~/solar-monitor'

echo "Deployment complete!"
