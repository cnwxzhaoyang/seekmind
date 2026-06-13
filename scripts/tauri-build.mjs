/**
 * @author MorningSun
 * @CreatedDate 2026/06/13
 * @Description SeekMind 跨平台 Tauri 构建脚本，统一处理 target、签名参数和本地 CLI 调用。
 */

import { npmExecCommand, runCommand } from "./seekmind-runtime-env.mjs";

const args = process.argv.slice(2);
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
