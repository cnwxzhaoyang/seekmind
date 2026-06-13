/**
 * @author MorningSun
 * @CreatedDate 2026/06/13
 * @Description SeekMind 跨平台 embedding warmup 脚本，统一模型缓存目录与镜像环境变量。
 */

import path from "node:path";

import { defaultPythonCommand, rootDir, runCommand } from "./seekmind-runtime-env.mjs";

const args = process.argv.slice(2);
const extraEnv = {};

if (args.includes("--mirror")) {
  extraEnv.HF_ENDPOINT = "https://hf-mirror.com";
}

await runCommand(
  process.env.SEEKMIND_PARSER_BIN || defaultPythonCommand(),
  [
    path.join(rootDir, "parser", "seekmind_parser", "__main__.py"),
    "warmup-embedding",
    "BAAI/bge-small-zh-v1.5",
  ],
  extraEnv,
);
