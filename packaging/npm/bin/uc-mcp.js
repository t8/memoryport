#!/usr/bin/env node
const { execFileSync } = require("child_process");
const path = require("path");
const bin = path.join(__dirname, "uc-mcp");
try {
  execFileSync(bin, process.argv.slice(2), { stdio: "inherit" });
} catch (err) {
  process.exit(err.status || 1);
}
