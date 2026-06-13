/**
 * @author MorningSun
 * @CreatedDate 2026/06/13
 * @Description SeekMind 跨平台 Tauri 开发启动脚本，统一管理开发态环境变量与启动参数。
 */

import { npmExecCommand, runCommand } from "./seekmind-runtime-env.mjs";

const args = process.argv.slice(2);
const extraEnv = {};
const tauriArgs = ["dev"];

if (args.includes("--devtools")) {
  extraEnv.SEEKMIND_OPEN_DEVTOOLS = "1";
}

if (args.includes("--trace-indexer")) {
  extraEnv.SEEKMIND_TRACE_INDEXER = "1";
}

if (args.includes("--first-launch")) {
  extraEnv.SEEKMIND_FORCE_FIRST_LAUNCH = "1";
}

if (args.includes("--reset-local-storage")) {
  tauriArgs.push("--", "--reset-local-storage");
}

await runCommand(npmExecCommand(), ["tauri", ...tauriArgs], extraEnv);
