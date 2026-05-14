#!/usr/bin/env node
// Postinstall script that downloads the platform-specific `evault`
// binary from the matching GitHub Release and unpacks it next to
// `bin/evault.js` so the `bin` field resolves correctly.
//
// The script runs at `npm install` time. It is deliberately
// dependency-free: no `node_modules/` lookups, no fetch shim — only
// Node core modules so the install path stays fast and robust.

"use strict";

const fs = require("fs");
const path = require("path");
const https = require("https");
const crypto = require("crypto");
const { execSync } = require("child_process");
const os = require("os");

const pkg = require("../package.json");
const VERSION = pkg.version;
const REPO = "stescobedo/hide-env-keys";

// Map (platform, arch) → (cargo target triple, archive ext, binary name).
const TARGETS = {
  "darwin-x64": {
    triple: "x86_64-apple-darwin",
    ext: "tar.xz",
    bin: "evault",
  },
  "darwin-arm64": {
    triple: "aarch64-apple-darwin",
    ext: "tar.xz",
    bin: "evault",
  },
  "linux-x64": {
    triple: "x86_64-unknown-linux-gnu",
    ext: "tar.xz",
    bin: "evault",
  },
  "win32-x64": {
    triple: "x86_64-pc-windows-msvc",
    ext: "zip",
    bin: "evault.exe",
  },
};

function platformKey() {
  return `${process.platform}-${process.arch}`;
}

function targetForCurrentHost() {
  const key = platformKey();
  const target = TARGETS[key];
  if (!target) {
    const supported = Object.keys(TARGETS).join(", ");
    throw new Error(
      `evault: no pre-built binary for ${key}. ` +
        `Supported platforms: ${supported}. ` +
        `To install from source: cargo install evault-cli.`
    );
  }
  return target;
}

function archiveBaseName(target) {
  return `evault-v${VERSION}-${target.triple}`;
}

function archiveUrl(target) {
  return `https://github.com/${REPO}/releases/download/v${VERSION}/${archiveBaseName(target)}.${target.ext}`;
}

function checksumUrl(target) {
  return `${archiveUrl(target)}.sha256`;
}

// `https.get` does not follow redirects; GitHub Releases serves a 302
// pointing at the S3-backed asset URL, so we follow up to 5 hops.
function httpGet(url, dest, hops = 5) {
  return new Promise((resolve, reject) => {
    if (hops <= 0) {
      reject(new Error(`evault: too many redirects fetching ${url}`));
      return;
    }
    const req = https.get(url, { headers: { "user-agent": "evault-npm-installer" } }, (res) => {
      if (
        res.statusCode &&
        res.statusCode >= 300 &&
        res.statusCode < 400 &&
        res.headers.location
      ) {
        res.resume();
        httpGet(res.headers.location, dest, hops - 1).then(resolve, reject);
        return;
      }
      if (res.statusCode !== 200) {
        res.resume();
        reject(
          new Error(
            `evault: HTTP ${res.statusCode} fetching ${url} ` +
              `(release v${VERSION} not yet published?)`
          )
        );
        return;
      }
      const file = fs.createWriteStream(dest);
      res.pipe(file);
      file.on("finish", () => file.close((err) => (err ? reject(err) : resolve())));
      file.on("error", reject);
    });
    req.on("error", reject);
  });
}

function fetchToString(url, hops = 5) {
  return new Promise((resolve, reject) => {
    if (hops <= 0) {
      reject(new Error(`evault: too many redirects fetching ${url}`));
      return;
    }
    const req = https.get(url, { headers: { "user-agent": "evault-npm-installer" } }, (res) => {
      if (
        res.statusCode &&
        res.statusCode >= 300 &&
        res.statusCode < 400 &&
        res.headers.location
      ) {
        res.resume();
        fetchToString(res.headers.location, hops - 1).then(resolve, reject);
        return;
      }
      if (res.statusCode !== 200) {
        res.resume();
        reject(new Error(`evault: HTTP ${res.statusCode} fetching ${url}`));
        return;
      }
      let body = "";
      res.setEncoding("utf8");
      res.on("data", (chunk) => (body += chunk));
      res.on("end", () => resolve(body));
    });
    req.on("error", reject);
  });
}

function sha256File(file) {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(file));
  return hash.digest("hex");
}

function extract(archivePath, ext, destDir) {
  // Use the host's archive tools rather than bundling a JS unpacker.
  // tar is available out of the box on macOS, Linux, and Windows 10+
  // (since 2018). 7z / Expand-Archive is the fallback on Windows for
  // zip — Node 18+ has `unzip`-like behaviour via `zlib`, but here we
  // keep it boring: `tar -xJf` for tar.xz, PowerShell's
  // Expand-Archive for zip.
  if (ext === "tar.xz") {
    execSync(`tar -xJf "${archivePath}" -C "${destDir}"`, { stdio: "inherit" });
  } else if (ext === "zip") {
    if (process.platform === "win32") {
      execSync(
        `powershell -NoProfile -Command "Expand-Archive -Force -Path '${archivePath}' -DestinationPath '${destDir}'"`,
        { stdio: "inherit" }
      );
    } else {
      // Fallback for non-Windows hosts that somehow got a zip target.
      execSync(`unzip -o "${archivePath}" -d "${destDir}"`, { stdio: "inherit" });
    }
  } else {
    throw new Error(`evault: unsupported archive extension '${ext}'`);
  }
}

async function main() {
  // Uninstall hook: clean up the binary before npm removes the package.
  // Best-effort only — npm ignores preuninstall failures by design.
  if (process.argv.includes("--uninstall")) {
    const target = TARGETS[platformKey()];
    if (target) {
      const binPath = path.join(__dirname, target.bin);
      if (fs.existsSync(binPath)) fs.unlinkSync(binPath);
    }
    return;
  }

  // Skip when running inside the source repo's own `npm install` (e.g.
  // local dev of this script). The release binary won't exist for an
  // unreleased version.
  if (process.env.EVAULT_SKIP_POSTINSTALL) {
    console.log("evault: EVAULT_SKIP_POSTINSTALL is set, skipping binary download");
    return;
  }

  const target = targetForCurrentHost();
  const binPath = path.join(__dirname, target.bin);
  if (fs.existsSync(binPath)) {
    console.log(`evault: binary already present at ${binPath}, skipping download`);
    return;
  }

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "evault-install-"));
  const archivePath = path.join(tmpDir, `${archiveBaseName(target)}.${target.ext}`);

  console.log(`evault: downloading v${VERSION} for ${target.triple}`);

  try {
    await httpGet(archiveUrl(target), archivePath);

    // Integrity check: the release pipeline publishes a sibling .sha256.
    const checksumBody = await fetchToString(checksumUrl(target));
    const expected = checksumBody.trim().split(/\s+/)[0].toLowerCase();
    const actual = sha256File(archivePath).toLowerCase();
    if (expected !== actual) {
      throw new Error(
        `evault: checksum mismatch for v${VERSION} ${target.triple}\n` +
          `  expected: ${expected}\n` +
          `  actual:   ${actual}`
      );
    }

    console.log(`evault: extracting to ${path.dirname(binPath)}`);
    extract(archivePath, target.ext, path.dirname(binPath));

    // The archive contains `evault[.exe]` plus README + LICENSE.
    // Leave the binary; tidy the rest.
    for (const sibling of ["README.md", "LICENSE"]) {
      const p = path.join(path.dirname(binPath), sibling);
      if (fs.existsSync(p)) fs.unlinkSync(p);
    }

    // On Unix, ensure the binary is executable.
    if (process.platform !== "win32") {
      fs.chmodSync(binPath, 0o755);
    }

    console.log(`evault: installed binary at ${binPath}`);
  } finally {
    // Best-effort cleanup of the tempdir.
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch (_e) {
      // ignore
    }
  }
}

main().catch((err) => {
  console.error(err.message || err);
  console.error("");
  console.error("If this persists, install from source instead:");
  console.error("  cargo install evault-cli");
  console.error("Or download the binary directly from:");
  console.error(`  https://github.com/${REPO}/releases/tag/v${VERSION}`);
  process.exit(1);
});
