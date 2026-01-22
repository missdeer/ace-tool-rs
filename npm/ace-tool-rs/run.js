#!/usr/bin/env node

const { spawn } = require("child_process");
const path = require("path");
const os = require("os");

const PLATFORMS = {
  "darwin-x64": "@ace-tool-rs/darwin-universal",
  "darwin-arm64": "@ace-tool-rs/darwin-universal",
  "linux-x64": "@ace-tool-rs/linux-x64",
  "linux-arm64": "@ace-tool-rs/linux-arm64",
  "win32-x64": "@ace-tool-rs/win32-x64",
  "win32-arm64": "@ace-tool-rs/win32-arm64",
};

function getBinaryPath() {
  const platformKey = `${process.platform}-${process.arch}`;
  const pkgName = PLATFORMS[platformKey];

  if (!pkgName) {
    console.error(`Unsupported platform: ${process.platform}-${process.arch}`);
    console.error("Supported platforms: " + Object.keys(PLATFORMS).join(", "));
    process.exit(1);
  }

  try {
    const pkgPath = require.resolve(`${pkgName}/package.json`);
    const binName = process.platform === "win32" ? "ace-tool-rs.exe" : "ace-tool-rs";
    return path.join(path.dirname(pkgPath), "bin", binName);
  } catch (e) {
    console.error(`Failed to find platform package: ${pkgName}`);
    console.error("This may happen if npm failed to install the optional dependency.");
    console.error("");
    console.error("Try reinstalling:");
    console.error("  npm install ace-tool-rs");
    console.error("");
    console.error("Or install the platform package directly:");
    console.error(`  npm install ${pkgName}`);
    process.exit(1);
  }
}

function run() {
  const binaryPath = getBinaryPath();
  const args = process.argv.slice(2);

  const child = spawn(binaryPath, args, {
    stdio: "inherit",
    env: process.env,
  });

  // Forward signals to child process
  const signals = ["SIGINT", "SIGTERM", "SIGHUP"];
  signals.forEach((signal) => {
    process.on(signal, () => {
      if (!child.killed) {
        child.kill(signal);
      }
    });
  });

  child.on("error", (error) => {
    console.error(`Failed to start ace-tool-rs: ${error.message}`);
    process.exit(1);
  });

  child.on("exit", (code, signal) => {
    if (signal) {
      process.exit(128 + (os.constants.signals[signal] || 0));
    }
    process.exit(code ?? 0);
  });
}

run();
