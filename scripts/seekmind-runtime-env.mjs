/**
 * @author MorningSun
 * @CreatedDate 2026/06/13
 * @Description SeekMind 跨平台开发脚本共享环境与命令执行封装。
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const currentFile = fileURLToPath(import.meta.url);
export const rootDir = path.resolve(path.dirname(currentFile), "..");

function normalizeWindowsEnv(env) {
  if (process.platform !== "win32") {
    return env;
  }

  const normalizedEnv = {};
  const seenKeys = new Set();
  let resolvedPathValue = undefined;

  for (const [key, value] of Object.entries(env)) {
    if (key.toLowerCase() === "path") {
      // 修复：Windows 上同时传 PATH / Path 给 child_process.spawn 会触发 EINVAL，这里统一收敛成 Path。
      resolvedPathValue = value;
      continue;
    }

    const normalizedKey = key.toLowerCase();
    if (seenKeys.has(normalizedKey)) {
      continue;
    }
    seenKeys.add(normalizedKey);
    normalizedEnv[key] = value;
  }

  if (resolvedPathValue !== undefined) {
    normalizedEnv.Path = resolvedPathValue;
  }

  return normalizedEnv;
}

export function seekMindRuntimeEnv(extraEnv = {}) {
  const inheritedPath = process.env.Path ?? process.env.PATH ?? "";
  const cargoBinDir = path.join(os.homedir(), ".cargo", "bin");
  const runtimePath = inheritedPath
    ? `${cargoBinDir}${path.delimiter}${inheritedPath}`
    : cargoBinDir;

  return normalizeWindowsEnv({
    ...process.env,
    Path: runtimePath,
    SEEKMIND_USE_PY_PARSER: "1",
    SEEKMIND_PARSER_SCRIPT: path.join(rootDir, "parser", "seekmind_parser", "__main__.py"),
    SEEKMIND_FASTEMBED_CACHE_DIR: path.join(rootDir, ".SeekMind-cache", "fastembed"),
    ...extraEnv,
  });
}

export function runCommand(command, args, extraEnv = {}) {
  return new Promise((resolve, reject) => {
    const useShell = process.platform === "win32" && command.toLowerCase().endsWith(".cmd");
    const child = spawn(command, args, {
      cwd: rootDir,
      stdio: "inherit",
      env: seekMindRuntimeEnv(extraEnv),
      // 修复：Windows 不能稳定地以 shell=false 直接启动 npx.cmd，改为按 .cmd 走 shell 启动。
      shell: useShell,
    });

    child.on("error", (error) => {
      reject(error);
    });

    child.on("exit", (code, signal) => {
      if (signal) {
        reject(new Error(`${command} exited with signal ${signal}`));
        return;
      }
      if (code !== 0) {
        reject(new Error(`${command} exited with code ${code}`));
        return;
      }
      resolve();
    });
  });
}

export function npmExecCommand() {
  return process.platform === "win32" ? "npx.cmd" : "npx";
}

function spawnSyncCommand(command, args = []) {
  try {
    return spawnSync(command, args, {
      cwd: rootDir,
      stdio: "ignore",
      env: seekMindRuntimeEnv(),
      shell: process.platform === "win32" && command.toLowerCase().endsWith(".cmd"),
    });
  } catch {
    return { status: 1, error: new Error("spawnSync unavailable") };
  }
}

function commandWorks(command, args = []) {
  const result = spawnSyncCommand(command, args);
  return !result.error && result.status === 0;
}

function candidateHasFastembed(candidate) {
  const result = spawnSyncCommand(candidate, ["-c", "import fastembed"]);
  return !result.error && result.status === 0;
}

function detectWindowsPythonCommand() {
  const localAppData = process.env.LOCALAPPDATA;
  const userProfile = process.env.USERPROFILE;
  const candidates = [];

  if (localAppData) {
    for (const version of ["Python314", "Python313", "Python312", "Python311", "Python310"]) {
      candidates.push(path.join(localAppData, "Programs", "Python", version, "python.exe"));
    }
  }

  if (userProfile) {
    candidates.push(path.join(userProfile, "Miniconda3", "python.exe"));
    candidates.push(path.join(userProfile, "anaconda3", "python.exe"));
  }

  for (const candidate of candidates) {
    if (!fs.existsSync(candidate)) {
      continue;
    }
    // 修复：Windows 打包不能继续命中 Store alias 的 python.exe，这里优先挑选已安装 fastembed 的真实解释器。
    if (candidateHasFastembed(candidate)) {
      return candidate;
    }
  }

  if (commandWorks("py", ["--version"])) {
    return "py";
  }

  return "python";
}

export function defaultPythonCommand() {
  return process.platform === "win32" ? detectWindowsPythonCommand() : "python3";
}
