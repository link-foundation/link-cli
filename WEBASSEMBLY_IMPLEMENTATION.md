# WebAssembly Implementation for link-cli

## Overview

This document describes the WebAssembly implementation of the `clink` CLI tool, enabling users to execute link manipulation operations directly in web browsers as requested in issue #12.

## Implementation Details

### 🔧 Technology Stack

- **Rust** - Core implementation language for WebAssembly
- **wasm-bindgen** - Rust/WebAssembly and JavaScript interop
- **wasm-pack** - Build tool for Rust-generated WebAssembly
- **web-sys** - Web API bindings
- **serde** - JSON serialization/deserialization

### 📁 Project Structure

```
├── src/
│   ├── lib.rs                 # Main WebAssembly interface
│   ├── query_processor.rs     # LiNo query processing logic
│   ├── links_operations.rs    # In-memory links storage
│   ├── lino_parser.rs        # LiNo protocol parser
│   └── utils.rs              # Utility functions
├── tests/
│   └── web.rs                # WebAssembly tests
├── www/
│   └── index.html            # Demo web application
├── .github/workflows/
│   └── wasm.yml              # CI/CD for WebAssembly builds
├── Cargo.toml                # Rust dependencies
├── package.json              # NPM package configuration
└── README-WASM.md            # WebAssembly documentation
```

### 🚀 Core Features

1. **WebAssembly Interface**
   - `Clink` class for browser/Node.js usage
   - JSON-based query execution
   - Version information and testing utilities

2. **LiNo Protocol Support**
   - Create: `() ((1 1))` - Create new links
   - Read: `((($i: $s $t)) (($i: $s $t)))` - Query existing links
   - Update: `((1: 1 1)) ((1: 1 2))` - Modify links
   - Delete: `((1 1)) ()` - Remove links

3. **Multiple Build Targets**
   - Web browsers (ES modules)
   - Node.js (CommonJS)
   - Bundlers (Webpack, Rollup, etc.)

### 🌐 Browser Integration

The WebAssembly version runs entirely client-side with:
- In-memory link storage
- Real-time query processing
- Interactive web demo at `/www/index.html`

### 📋 API Reference

```javascript
import init, { Clink } from './pkg/clink_wasm.js';

// Initialize WebAssembly
await init();

// Create instance
const clink = new Clink();

// Execute queries
const options = JSON.stringify({
    db: "memory",
    changes: true,
    after: true
});

const result = clink.execute("() ((1 1))", options);
const parsed = JSON.parse(result);

console.log(parsed.output);
```

### 🧪 Testing Strategy

1. **Rust Unit Tests** - Core functionality testing
2. **WebAssembly Tests** - Browser integration testing
3. **CI/CD Pipeline** - Automated builds and testing

### 📦 Distribution

The WebAssembly version is distributed as:
- **NPM Package**: `clink-wasm` for Node.js usage
- **Web Package**: Direct browser import via ES modules
- **GitHub Pages**: Live demo deployment

## Protocols.Lino Compatibility

The implementation includes a comprehensive LiNo parser that supports:
- Simple references (numbers)
- Empty links `()`
- Links with source/target `(1 2)`
- Links with IDs `(3: 1 2)`
- Variable patterns `$i`, `$s`, `$t`
- Wildcard patterns `*`

Tests ensure compatibility with the Rust implementation at:
https://github.com/linksplatform/Protocols.Lino/blob/03be561ef9612fe7a86ed9f2ad964827cc6b4df5/rust/src/lib.rs

## Version Update

The project version has been updated to `2.3.0` to reflect the addition of WebAssembly support:
- C# project: `Foundation.Data.Doublets.Cli.csproj`
- Rust project: `Cargo.toml`
- NPM package: `package.json`

## Build Instructions

### Prerequisites
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Add WebAssembly target
rustup target add wasm32-unknown-unknown
```

### Building
```bash
# Build for web browsers
npm run build

# Build for Node.js
npm run build:nodejs

# Build for bundlers
npm run build:bundler

# Build all targets
npm run build:all
```

### Testing
```bash
# Run Rust tests
cargo test

# Run WebAssembly tests
npm test

# Serve demo locally
npm run serve
# Open http://localhost:8000/www/
```

## Future Enhancements

- [ ] Persistent storage using IndexedDB
- [ ] Worker thread support for better performance
- [ ] Full doublets library integration
- [ ] Advanced query optimization
- [ ] Streaming query execution

## Conclusion

This WebAssembly implementation successfully addresses issue #12 by providing a browser-compatible version of the `clink` CLI tool. Users can now execute link manipulation operations directly in web browsers without requiring server-side infrastructure or .NET runtime installation.