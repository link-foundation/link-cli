# Clink WebAssembly Version

This is the WebAssembly port of the `clink` CLI tool, allowing you to execute link manipulation operations directly in your browser.

## 🚀 Quick Start

### Using in Browser

1. Build the project:
```bash
npm run build
```

2. Serve the demo:
```bash
npm run serve
```

3. Open http://localhost:8000/www/ in your browser

### Using in Node.js

1. Build for Node.js:
```bash
npm run build:nodejs
```

2. Use in your Node.js project:
```javascript
import { Clink } from './pkg-node/clink_wasm.js';

const clink = new Clink();
const options = JSON.stringify({
    db: "memory",
    trace: false,
    before: false,
    changes: true,
    after: true
});

const result = clink.execute("() ((1 1))", options);
const parsed = JSON.parse(result);
console.log(parsed.output);
```

### Using with bundlers (Webpack, Rollup, etc.)

1. Build for bundlers:
```bash
npm run build:bundler
```

2. Import in your JavaScript:
```javascript
import init, { Clink } from 'clink-wasm';

async function run() {
    await init();
    const clink = new Clink();
    // ... use clink
}

run();
```

## 📖 API Reference

### `Clink` class

#### Constructor
```javascript
const clink = new Clink();
```

#### Methods

##### `execute(query: string, options: string): string`
Execute a LiNo query with the specified options.

**Parameters:**
- `query`: The LiNo query string
- `options`: JSON string with execution options

**Options format:**
```javascript
{
    "db": "memory",        // Database path (always "memory" for WASM)
    "trace": false,        // Enable trace output
    "structure": null,     // Structure ID (not implemented yet)
    "before": false,       // Show state before changes
    "changes": true,       // Show the changes made
    "after": true          // Show state after changes
}
```

**Returns:** JSON string with result:
```javascript
{
    "success": true,
    "output": "() ((1: 1 1))\n(1: 1 1)",
    "error": null
}
```

##### `version(): string` (static)
Get the version of the WASM module.

##### `test(): boolean` (static)
Test if WebAssembly is working correctly.

## 🔗 Supported LiNo Operations

### Create Links
```javascript
// Create single link
clink.execute("() ((1 1))", options);

// Create multiple links
clink.execute("() ((1 1) (2 2))", options);
```

### Read Links
```javascript
// Read all links
clink.execute("((($i: $s $t)) (($i: $s $t)))", options);

// Read with shorter syntax
clink.execute("((($i:)) (($i:)))", options);
```

### Update Links
```javascript
// Update single link
clink.execute("((1: 1 1)) ((1: 1 2))", options);

// Update multiple links
clink.execute("((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))", options);
```

### Delete Links
```javascript
// Delete single link
clink.execute("((1 2)) ()", options);

// Delete multiple links
clink.execute("((1 2) (2 1)) ()", options);

// Delete all links
clink.execute("((* *)) ()", options);
```

## 🧪 Testing

Run tests in headless browsers:
```bash
npm test
npm run test:firefox
```

## 🏗️ Building

Build all variants:
```bash
npm run build:all
```

Individual builds:
```bash
npm run build        # Web target
npm run build:nodejs # Node.js target
npm run build:bundler # Bundler target
```

## 🌐 Browser Compatibility

The WebAssembly version works in all modern browsers that support WebAssembly:
- Chrome 57+
- Firefox 52+
- Safari 11+
- Edge 16+

## 📝 Notes

- This WebAssembly version uses an in-memory storage system
- Database files are not persistent across sessions in the browser
- All operations are executed client-side
- Performance may vary compared to the native C# version

## 🔮 Future Enhancements

- [ ] Persistent storage using IndexedDB
- [ ] Worker thread support for better performance
- [ ] Full Protocols.Lino compatibility
- [ ] Advanced query optimization
- [ ] Streaming query execution