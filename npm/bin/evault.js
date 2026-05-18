#!/usr/bin/env node
// Wrapper that execs the platform-specific `evault` binary provided by an
// optional dependency package. The wrapper preserves stdio, exit code, and
// signal forwarding so users experience `evault` as a transparent passthrough.

"use strict";

const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");

const BIN_NAME = process.platform === "win32" ? "evault.exe" : "evault";
const PACKAGE_BY_PLATFORM = {
  "darwin-arm64": "evault-cli-darwin-arm64",
  "darwin-x64": "evault-cli-darwin-x64",
  "linux-x64": "evault-cli-linux-x64",
  "win32-x64": "evault-cli-win32-x64",
};

function resolveBinary() {
  const key = `${process.platform}-${process.arch}`;
  const packageName = PACKAGE_BY_PLATFORM[key];
  if (!packageName) {
    throw new Error(
      `unsupported platform ${key}. Supported platforms: ${Object.keys(
        PACKAGE_BY_PLATFORM
      ).join(", ")}`
    );
  }
  let packageJson;
  try {
    packageJson = require.resolve(`${packageName}/package.json`);
  } catch (_err) {
    throw new Error(
      `optional package ${packageName} is not installed. Reinstall with optional dependencies enabled, or install from source with \`cargo install evault-cli\`.`
    );
  }
  return path.join(path.dirname(packageJson), "bin", BIN_NAME);
}

let binPath;
try {
  binPath = resolveBinary();
} catch (err) {
  console.error(`evault: ${err.message}`);
  process.exit(1);
}

if (!fs.existsSync(binPath)) {
  console.error(
    `evault: platform package is installed but binary is missing at ${binPath}.\n` +
      `Reinstall the package, or install from source with \`cargo install evault-cli\`.`
  );
  process.exit(1);
}

const child = spawn(binPath, process.argv.slice(2), {
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
