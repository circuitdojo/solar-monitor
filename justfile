# Solar Monitor task runner — `just --list` for an overview.
#
# Machine-specific values (your Pi's host/user) go in a gitignored .env:
#   SOLAR_PI=user@your-pi.local
#   SOLAR_PI_URL=http://your-pi.local:8080

set dotenv-load

pi := env_var_or_default("SOLAR_PI", "pi@solar-pi.local")
pi_url := env_var_or_default("SOLAR_PI_URL", "http://solar-pi.local:8080")

# List available recipes
default:
    @just --list

# Build the web frontend (required before serve/deploy)
web:
    cd web && npm run build

# Run the dev server locally (8080 is taken on the dev machine)
serve port="8090": web
    cargo run -p solar-monitor -- --serve --port {{port}}

# Regenerate TypeScript bindings from the Rust contracts
types:
    cargo run -p contracts --bin export_types

# Run the full test suite
test:
    cargo test --workspace

# Format + lint (run before every commit)
lint:
    cargo fmt
    cargo clippy --workspace --all-features

# Cross-compile the self-contained Pi binary (web UI embedded)
build-pi: web
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
      cargo build --release --target aarch64-unknown-linux-gnu \
      -p solar-monitor --features solar-monitor-api/embed-frontend
    @strings target/aarch64-unknown-linux-gnu/release/solar-monitor | grep -q "assets/index-" \
      || { echo "ERROR: frontend not embedded in binary"; exit 1; }

# Build + deploy to the production Pi and verify it serves
deploy: build-pi
    scp target/aarch64-unknown-linux-gnu/release/solar-monitor {{pi}}:/tmp/
    ssh {{pi}} 'sudo install -m755 /tmp/solar-monitor /usr/local/bin/ && sudo systemctl restart solar-monitor'
    sleep 2
    curl -sf -m 5 {{pi_url}}/api/v1/health
    @curl -sf -m 5 -o /dev/null -w "\ndashboard: %{http_code}\n" {{pi_url}}/

# Tail the service log on the Pi
pi-logs:
    ssh {{pi}} journalctl -u solar-monitor -n 50 --no-pager

# Service status on the Pi
pi-status:
    ssh {{pi}} systemctl status solar-monitor --no-pager

# Cross-compile the self-contained FreeBSD binary (needs the sysroot — see README)
build-freebsd: web
    cargo build --release --target x86_64-unknown-freebsd \
      -p solar-monitor --features solar-monitor-api/embed-frontend
