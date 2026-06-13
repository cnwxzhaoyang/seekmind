/**
 * @author MorningSun
 * @CreatedDate 2026/06/13
 * @Description SeekMind 跨平台 parser sidecar 打包脚本，统一构建 OCR helper、PyInstaller 产物与 fastembed 资源。
 */

import fs from "node:fs/promises";
import path from "node:path";
import { spawn } from "node:child_process";

import {
  defaultPythonCommand,
  npmExecCommand,
  rootDir,
  runCommand,
} from "./seekmind-runtime-env.mjs";

const args = process.argv.slice(2);
const pythonCommand = process.env.SEEKMIND_PARSER_BIN || defaultPythonCommand();
const fastembedCacheDir =
  process.env.SEEKMIND_FASTEMBED_CACHE_DIR ||
  path.join(rootDir, ".SeekMind-cache", "fastembed");
const appResourceDir = path.join(rootDir, "src-tauri", "app-resources");
const resourceDir = path.join(rootDir, "src-tauri", "resources");
const buildDir = path.join(rootDir, ".seekmind-build", "parser-sidecar");
const distDir = path.join(buildDir, "dist");
const workDir = path.join(buildDir, "build");
const specDir = path.join(buildDir, "spec");
const ocrDir = path.join(appResourceDir, "ocr");
const parserBaseName = "seekmind-parser";
const visionOcrBaseName = "seekmind-vision-ocr";
const hostOs = process.platform === "darwin" ? "macos" : process.platform === "win32" ? "windows" : process.platform;

async function ensureDir(dir) {
  await fs.mkdir(dir, { recursive: true });
}

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath);
    return true;
  } catch {
    return false;
  }
}

async function removeIfExists(targetPath) {
  if (await pathExists(targetPath)) {
    await fs.rm(targetPath, { recursive: true, force: true });
  }
}

async function copyIfExists(source, target) {
  if (await pathExists(source)) {
    await removeIfExists(target);
    await fs.cp(source, target, { recursive: true });
  }
}

async function resolveRustHostTriple() {
  let stdout = "";
  await new Promise((resolve, reject) => {
    const child = spawn("rustc", ["-vV"], {
      cwd: rootDir,
      env: process.env,
      shell: false,
      stdio: ["ignore", "pipe", "inherit"],
    });
    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code !== 0) {
        reject(new Error(`rustc -vV exited with code ${code}`));
        return;
      }
      resolve();
    });
  });

  const hostLine = stdout
    .split("\n")
    .find((line) => line.trim().startsWith("host:"));
  if (!hostLine) {
    throw new Error("failed to resolve rust host triple");
  }
  return hostLine.split(":").slice(1).join(":").trim();
}

function targetOsFromTriple(targetTriple) {
  if (!targetTriple) {
    return hostOs;
  }
  if (targetTriple.includes("windows")) {
    return "windows";
  }
  if (targetTriple.includes("apple-darwin")) {
    return "macos";
  }
  if (targetTriple.includes("linux")) {
    return "linux";
  }
  return "unknown";
}

function assertHostCanBuildTargetSidecar(targetTriple) {
  const targetOs = targetOsFromTriple(targetTriple);
  if (targetOs === "unknown") {
    throw new Error(`[SeekMind] unsupported target triple for parser sidecar: ${targetTriple}`);
  }
  if (targetOs !== hostOs) {
    throw new Error(
      `[SeekMind] parser sidecar build requires a ${targetOs} host, current host is ${hostOs}; target=${targetTriple}`,
    );
  }
}

async function buildVisionOcrHelper(targetTriple) {
  if (targetOsFromTriple(targetTriple) !== "macos") {
    console.info("[SeekMind] skip bundled Vision OCR helper for non-macOS target");
    await removeIfExists(ocrDir);
    return;
  }

  if (process.platform !== "darwin") {
    console.info("[SeekMind] skip bundled Vision OCR helper build on non-macOS host");
    return;
  }

  console.info("[SeekMind] building bundled Vision OCR helper...");
  await removeIfExists(ocrDir);
  await runCommand("cargo", [
    "build",
    "--manifest-path",
    path.join(rootDir, "src-tauri", "Cargo.toml"),
    "--release",
    "--bin",
    visionOcrBaseName,
  ]);

  const helperBin = path.join(rootDir, "src-tauri", "target", "release", visionOcrBaseName);
  if (!(await pathExists(helperBin))) {
    throw new Error(`Vision OCR helper build failed: ${helperBin} not found`);
  }

  await ensureDir(ocrDir);
  await fs.copyFile(helperBin, path.join(ocrDir, "vision-ocr"));
  console.info(`[SeekMind] bundled Vision OCR helper: ${path.join(ocrDir, "vision-ocr")}`);
}

async function ensurePyInstaller() {
  try {
    await runCommand(pythonCommand, ["-m", "PyInstaller", "--version"]);
  } catch {
    console.info("[SeekMind] installing PyInstaller...");
    await runCommand(pythonCommand, ["-m", "pip", "install", "pyinstaller"]);
  }
}

async function buildParserSidecar(outputName) {
  console.info(`[SeekMind] building parser sidecar bundle ${outputName}...`);
  await ensureDir(resourceDir);
  await ensureDir(appResourceDir);
  await ensureDir(distDir);
  await ensureDir(workDir);
  await ensureDir(specDir);

  await runCommand(
    pythonCommand,
    ["-m", "pip", "install", "-r", path.join(rootDir, "parser", "requirements.txt")],
  );
  await ensurePyInstaller();

  await runCommand(
    pythonCommand,
    [
      "-m",
      "PyInstaller",
      "--noconfirm",
      "--clean",
      "--onedir",
      "--name",
      parserBaseName,
      "--distpath",
      distDir,
      "--workpath",
      workDir,
      "--specpath",
      specDir,
      path.join(rootDir, "parser", "seekmind_parser", "__main__.py"),
    ],
  );

  const builtSidecarDir = path.join(distDir, parserBaseName);
  const bundledOutputDir = path.join(appResourceDir, outputName);
  await copyIfExists(builtSidecarDir, bundledOutputDir);
  console.info(`[SeekMind] built parser sidecar: ${bundledOutputDir}`);
}

async function bundleFastembedCache() {
  const bundledFastembedDir = path.join(appResourceDir, "fastembed");
  await removeIfExists(path.join(appResourceDir, "fastembed-cache.tar.gz"));
  await removeIfExists(bundledFastembedDir);

  if (!(await pathExists(fastembedCacheDir))) {
    console.info(
      `[SeekMind] FastEmbed cache not found at ${fastembedCacheDir}; semantic model may need runtime download`,
    );
    return;
  }

  await fs.cp(fastembedCacheDir, bundledFastembedDir, { recursive: true });
  console.info(`[SeekMind] bundled FastEmbed cache dir: ${bundledFastembedDir}`);
}

async function runTauriBuild() {
  const tauriArgs = ["tauri", "build"];
  const extraEnv = {};

  const targetArgIndex = args.indexOf("--target");
  if (targetArgIndex >= 0 && args[targetArgIndex + 1]) {
    tauriArgs.push("--target", args[targetArgIndex + 1]);
  }

  if (args.includes("--apple-adhoc")) {
    extraEnv.APPLE_SIGNING_IDENTITY = "-";
  }

  await runCommand(npmExecCommand(), tauriArgs, extraEnv);
}

const requestedTarget = (() => {
  const targetArgIndex = args.indexOf("--target");
  if (targetArgIndex >= 0 && args[targetArgIndex + 1]) {
    return args[targetArgIndex + 1];
  }
  return null;
})();

const effectiveTarget = requestedTarget || (await resolveRustHostTriple());
assertHostCanBuildTargetSidecar(effectiveTarget);
const outputName = `${parserBaseName}-${effectiveTarget}`;

await buildVisionOcrHelper(effectiveTarget);
await buildParserSidecar(outputName);
await bundleFastembedCache();

if (args.includes("--tauri-build")) {
  await runTauriBuild();
}
