#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const raw = process.argv[2] || "";
const version = raw.startsWith("v") ? raw.slice(1) : raw;
const semver = /^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/;

if (!semver.test(version)) {
  console.error(
    `release version '${raw}' is not valid SemVer. Use MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-prerelease.`
  );
  process.exit(1);
}

const root = path.resolve(__dirname, "..");
const cargoToml = path.join(root, "Cargo.toml");
replaceFile(cargoToml, (text) =>
  text.replace(
    /(\[workspace\.package\][\s\S]*?version = ")[^"]+(")/,
    `$1${version}$2`
  )
);

for (const crate of fs.readdirSync(path.join(root, "crates"))) {
  const file = path.join(root, "crates", crate, "Cargo.toml");
  if (!fs.existsSync(file)) continue;
  replaceFile(file, (text) =>
    text.replace(
      /(evault-[a-z-]+ = \{ path = "[^"]+", version = ")[^"]+(" \})/g,
      `$1${version}$2`
    )
  );
}

const npmRoot = path.join(root, "npm");
const npmPackage = path.join(npmRoot, "package.json");
if (fs.existsSync(npmPackage)) {
  updateJson(npmPackage, (pkg) => {
    pkg.version = version;
    for (const name of Object.keys(pkg.optionalDependencies || {})) {
      pkg.optionalDependencies[name] = version;
    }
  });
}

const npmPackages = path.join(npmRoot, "packages");
if (fs.existsSync(npmPackages)) {
  for (const dir of fs.readdirSync(npmPackages)) {
    const file = path.join(npmPackages, dir, "package.json");
    if (!fs.existsSync(file)) continue;
    updateJson(file, (pkg) => {
      pkg.version = version;
    });
  }
}

console.log(`synced release version ${version}`);

function replaceFile(file, transform) {
  const before = fs.readFileSync(file, "utf8");
  const after = transform(before);
  if (after !== before) fs.writeFileSync(file, after);
}

function updateJson(file, mutator) {
  const before = fs.readFileSync(file, "utf8");
  const pkg = JSON.parse(before);
  const semanticBefore = JSON.stringify(pkg);
  mutator(pkg);
  if (JSON.stringify(pkg) === semanticBefore) return;
  fs.writeFileSync(file, `${JSON.stringify(pkg, null, 2)}\n`);
}
