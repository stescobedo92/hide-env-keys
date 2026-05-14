#!/usr/bin/env node
// Wrapper that execs the platform-specific `evault` binary placed
// alongside this script by `install.js`. The wrapper preserves the
// process's stdio, exit code, and signal forwarding so the user
// experiences `evault` as a transparent passthrough.

"use strict";

const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");

const BIN_NAME = process.platform === "win32" ? "evault.exe" : "evault";
const BIN_PATH = path.join(__dirname, BIN_NAME);

if (!fs.existsSync(BIN_PATH)) {
  console.error(
    `evault: binary not found at ${BIN_PATH}.\n` +
      `The postinstall step may have failed — try \`npm rebuild evault\`,\n` +
      `or install from source: \`cargo install evault-cli\`.`
  );
  process.exit(1);
}

const child = spawn(BIN_PATH, process.argv.slice(2), {
  stdio: "inherit",
  // The Rust binary owns the terminal lifecycle (raw mode, alternate
  // screen). Passing the parent stdio through unchanged is essential
  // for the TUI to render correctly.
});

// Forward common signals so Ctrl-C in the parent kills the child
// cleanly. Without this, the parent's signal handler may exit while
// the child is still in raw mode, leaving the terminal corrupted.
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(sig, () => {
    if (!child.killed) child.kill(sig);
  });
}

child.on("exit", (code, signal) => {
  if (signal) {
    // Mirror the shell convention: 128 + signal number.
    const SIGNUMS = { SIGHUP: 1, SIGINT: 2, SIGTERM: 15 };
    process.exit(128 + (SIGNUMS[signal] || 0));
  }
  process.exit(code === null ? 1 : code);
});

child.on("error", (err) => {
  console.error(`evault: failed to spawn binary: ${err.message}`);
  process.exit(1);
});
