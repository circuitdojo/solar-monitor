## Cross-Compilation

### Build and Deploy

### Building only

1. Install tools:

```
sudo apt install gcc-aarch64-linux-gnu libc6-dev-arm64-cross
```

2. Set environment variables:

```
export PKG_CONFIG_ALLOW_CROSS=1
export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
```

3. Build for aarch64:

```
cargo build --target=aarch64-unknown-linux-gnu
```
