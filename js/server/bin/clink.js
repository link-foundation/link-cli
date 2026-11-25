#!/usr/bin/env node
// The `clink` executable: runs src/main.js with the command line and exits
// with its status, as the Rust `clink` binary does.
import { main } from '../src/main.js';
import { quietFfiWarning } from '../src/store-lock.js';

quietFfiWarning();
process.exitCode = await main(process.argv.slice(2));
