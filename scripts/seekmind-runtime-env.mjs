/**
 * @author MorningSun
 * @CreatedDate 2026/06/13
 * @Description SeekMind 跨平台开发脚本共享环境与命令执行封装。
 */

import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const currentFile = fileURLToPath(import.meta.url);
export const rootDir = path.resolve(path.dirname(currentFile), "..");

export function seekMindRuntimeEnv(extraEnv = {}) {
  return {
    ...process.env,
    SEEKMIND_USE_PY_PARSER: "1",
    SEEKMIND_PARSER_SCRIPT: path.join(rootDir, "parser", "seekmind_parser", "__main__.py"),
    SEEKMIND_FASTEMBED_CACHE_DIR: path.join(rootDir, ".SeekMind-cache", "fastembed"),
    ...extraEnv,
  };
}

export function runCommand(command, args, extraEnv = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: rootDir,
      stdio: "inherit",
      env: seekMindRuntimeEnv(extraEnv),
      shell: false,
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

export function defaultPythonCommand() {
  return process.platform === "win32" ? "python" : "python3";
}
