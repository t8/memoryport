#!/usr/bin/env node
const { execSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const https = require("https");

const REPO = "t8/memoryport";
const VERSION = require("./package.json").version;

const PLATFORM_MAP = {
  darwin: "macos",
  linux: "linux",
};

const ARCH_MAP = {
  x64: "x64",
  arm64: "arm64",
};

const os = PLATFORM_MAP[process.platform];
const arch = ARCH_MAP[process.arch];

if (!os || !arch) {
  console.error(`Unsupported platform: ${process.platform}-${process.arch}`);
  process.exit(1);
}

const binDir = path.join(__dirname, "bin");
const url = `https://github.com/${REPO}/releases/download/v${VERSION}/memoryport-${os}-${arch}.tar.gz`;

console.log(`Downloading Memoryport binaries for ${os}-${arch}...`);

const tmpFile = path.join(__dirname, "uc.tar.gz");

// Download using curl (available on macOS and Linux)
try {
  execSync(`curl -fsSL "${url}" -o "${tmpFile}"`, { stdio: "inherit" });
  fs.mkdirSync(binDir, { recursive: true });
  execSync(`tar -xzf "${tmpFile}" -C "${binDir}"`, { stdio: "inherit" });
  fs.unlinkSync(tmpFile);

  // Make binaries executable
  for (const bin of ["uc", "uc-mcp", "uc-proxy", "uc-server"]) {
    const binPath = path.join(binDir, bin);
    if (fs.existsSync(binPath)) {
      fs.chmodSync(binPath, 0o755);
    }
  }

  console.log("Memoryport installed successfully.");
  console.log("Run 'uc init' to complete setup.");
} catch (err) {
  console.error("Failed to download binaries:", err.message);
  console.error(`Download manually from: ${url}`);
  process.exit(1);
}
