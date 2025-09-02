# EG4 Monitor - Codebase Architecture

## Overview

EG4 Monitor is a Rust-based system for communicating with EG4 solar inverters using the PI30 protocol. The project implements a WebSocket-to-TCP bridge architecture that enables web browsers to communicate with EG4 devices over TCP/IP networks.

## High-Level Architecture

The system follows a three-tier architecture:

```
Web Browser (WASM Client) ↔ WebSocket ↔ Bridge Server ↔ TCP ↔ EG4 Inverter
```

### Core Components

1. **Bridge Server** (`bridge/`) - Rust TCP server that acts as a WebSocket-to-TCP gateway
2. **WASM Client** (`wasm-client/`) - WebAssembly library for browser-based communication
3. **Workspace Root** - Cargo workspace configuration managing shared dependencies

## Technology Stack

### Core Technologies
- **Language**: Rust (Edition 2024)
- **Async Runtime**: Tokio for asynchronous networking
- **WebSocket**: tokio-tungstenite for WebSocket server implementation
- **WebAssembly**: wasm-bindgen for browser integration
- **Serialization**: serde with JSON for message formatting

### Key Dependencies
- **Networking**: tokio, tokio-tungstenite, futures-util
- **CLI**: clap for command-line argument parsing
- **Logging**: tracing, tracing-subscriber for structured logging
- **Error Handling**: anyhow for error management
- **WebAssembly**: wasm-bindgen, web-sys, js-sys
- **Utilities**: hex for hexadecimal encoding, serde_json for JSON processing

## Project Structure

```
eg4-monitor/
├── Cargo.toml                 # Workspace configuration
├── bridge/                    # WebSocket-to-TCP bridge server
│   ├── Cargo.toml            # Bridge dependencies
│   ├── README.md             # Bridge documentation
│   ├── src/main.rs           # Bridge implementation (~270 lines)
│   └── test_connection.py    # Python test client
└── wasm-client/              # WebAssembly client library
    ├── Cargo.toml            # WASM dependencies
    └── src/lib.rs            # WASM client implementation (~190 lines)
```

## Protocol Implementation

### PI30 Protocol
The system implements the PI30 protocol for EG4 inverter communication:
- **CRC Calculation**: Custom CRC-16 implementation for message integrity
- **Command Format**: `<command><CRC><CR>` structure
- **Supported Commands**: QID, Q1, QPIRI, QPIWS, QPGS0, QBMS
- **Default Connection**: 192.168.10.7:8000 (configurable)

### Message Format
JSON-based message structure for WebSocket communication:
```json
{
  "type": "command|response|error|connected",
  "command": "PI30_COMMAND",
  "response": "device_response",
  "error": "error_message"
}
```

## Key Architectural Patterns

### 1. Workspace Organization
- Cargo workspace with shared dependency management
- Centralized version and metadata configuration
- Cross-component dependency sharing

### 2. Async/Await Architecture
- Tokio-based async runtime throughout
- Non-blocking I/O for all network operations
- Concurrent connection handling

### 3. Bridge Pattern
- Clean separation between WebSocket and TCP protocols
- Protocol translation and message forwarding
- Error propagation across protocol boundaries

### 4. WebAssembly Integration
- Browser-compatible WASM module
- JavaScript interop through wasm-bindgen
- Client-side WebSocket management

### 5. Error Handling Strategy
- anyhow for error propagation
- Structured logging with tracing
- Graceful connection failure handling

## Development Commands

### Build Commands
```bash
# Build entire workspace
cargo build

# Build release version
cargo build --release

# Build specific component
cargo build -p eg4-bridge
cargo build -p eg4-wasm-client

# Build WASM client for web
cd wasm-client
wasm-pack build --target web
```

### Testing Commands
```bash
# Run Rust tests
cargo test

# Test bridge connectivity
cd bridge
cargo run -- --test --eg4-host 192.168.10.7:8000

# Python integration test
cd bridge
pip install websockets
python test_connection.py
```

### Development Server
```bash
# Start bridge server (default: localhost:8080)
cd bridge
cargo run

# Custom configuration
cargo run -- --eg4-host 192.168.1.100:8000 --bind 0.0.0.0:9090
```

### Linting and Formatting
```bash
# Format code
cargo fmt

# Run clippy lints
cargo clippy

# Check without building
cargo check
```

## Configuration and Deployment

### Bridge Configuration
- **EG4 Host**: Configurable via `--eg4-host` (default: 192.168.10.7:8000)
- **WebSocket Bind**: Configurable via `--bind` (default: 127.0.0.1:8080)
- **Test Mode**: `--test` flag for connection validation

### WASM Client Configuration
- **Bridge URL**: Configurable WebSocket endpoint (default: ws://localhost:8080)
- **Browser Integration**: Direct instantiation in web applications

## Testing Approach

### Unit Testing
- Rust unit tests for protocol implementation
- CRC calculation verification
- Message serialization/deserialization

### Integration Testing
- Python WebSocket client for end-to-end testing
- Connection stability testing
- Command response validation

### Manual Testing
- Built-in connection test mode in bridge
- Command-line test utilities
- Browser console integration

## Key Conventions

### Code Style
- Rust 2024 edition features
- Async/await throughout for I/O operations
- Structured error handling with anyhow
- Comprehensive logging with tracing

### Naming Conventions
- Snake_case for Rust identifiers
- PascalCase for types and structs
- Descriptive function names reflecting PI30 protocol

### Dependencies Management
- Workspace-level dependency sharing
- Feature flags for conditional compilation
- Minimal dependency footprint

### Error Handling
- Result<T, E> pattern throughout
- Context-rich error messages
- Graceful degradation for network failures

## Architecture Strengths

1. **Separation of Concerns**: Clean separation between bridge, protocol, and client
2. **Protocol Abstraction**: PI30 protocol encapsulated in dedicated struct
3. **Async Performance**: Non-blocking I/O throughout the stack
4. **Web Compatibility**: WASM enables browser-based monitoring
5. **Extensibility**: Command pattern allows easy protocol extension
6. **Testing**: Comprehensive test utilities for validation

## Development Considerations

- **Network Dependencies**: Requires reliable network connectivity to EG4 devices
- **Protocol Limitations**: Some EG4 models may not support all PI30 commands
- **WebAssembly Constraints**: Browser security restrictions on network access
- **Concurrent Connections**: Single TCP connection per bridge instance
- **Error Recovery**: Manual reconnection required for dropped connections