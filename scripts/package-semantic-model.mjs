/**
 * @author MorningSun
 * @CreatedDate 2026/06/21
 * @Description 生成语义模型 GitHub Release 资源包和 SHA-256 校验文件。
 */

import fs from "node:fs/promises";
import { createHash } from "node:crypto";
import path from "node:path";
import { spawn } from "node:child_process";

import { rootDir } from "./seekmind-runtime-env.mjs";

const modelCacheDirName = "models--Qdrant--bge-small-zh-v1.5";
const fastembedCacheDir =
  process.env.SEEKMIND_FASTEMBED_CACHE_DIR ||
  path.join(rootDir, ".SeekMind-cache", "fastembed");
const outputDir = path.join(rootDir, ".seekmind-build", "semantic-models");
const archiveName = "fastembed-cache.tar.gz";
const archivePath = path.join(outputDir, archiveName);
const shaPath = `${archivePath}.sha256`;

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

function runPipeline(tarArgs, targetPath) {
  return new Promise((resolve, reject) => {
    const tar = spawn("tar", tarArgs, {
      cwd: fastembedCacheDir,
      env: {
        ...process.env,
        COPYFILE_DISABLE: "1",
      },
      stdio: ["ignore", "pipe", "inherit"],
    });
    const gzip = spawn("gzip", ["-n", "-c"], {
      cwd: fastembedCacheDir,
      stdio: ["pipe", "pipe", "inherit"],
    });

    const chunks = [];
    tar.stdout.pipe(gzip.stdin);
    gzip.stdout.on("data", (chunk) => chunks.push(chunk));
    tar.on("error", reject);
    gzip.on("error", reject);
    tar.on("exit", (code) => {
      if (code !== 0) {
        reject(new Error(`tar exited with code ${code}`));
      }
    });
    gzip.on("exit", async (code) => {
      if (code !== 0) {
        reject(new Error(`gzip exited with code ${code}`));
        return;
      }
      await fs.writeFile(targetPath, Buffer.concat(chunks));
      resolve();
    });
  });
}

async function sha256File(targetPath) {
  const data = await fs.readFile(targetPath);
  return createHash("sha256").update(data).digest("hex");
}

const modelCacheDir = path.join(fastembedCacheDir, modelCacheDirName);
if (!(await pathExists(modelCacheDir))) {
  throw new Error(
    `[SeekMind] FastEmbed cache missing: ${modelCacheDir}. Run npm run semantic:warmup:mirror first.`,
  );
}

await fs.mkdir(outputDir, { recursive: true });
await removeIfExists(archivePath);
await removeIfExists(shaPath);

// 修复：模型改为 GitHub Release 下载，这里只打包 fastembed 运行时真正需要的缓存目录，避免混入安装资源。
await runPipeline(["-cf", "-", "CACHEDIR.TAG", modelCacheDirName], archivePath);
const sha256 = await sha256File(archivePath);
const stat = await fs.stat(archivePath);
await fs.writeFile(shaPath, `${sha256}  ${archiveName}\n`, "utf8");

console.info(`[SeekMind] semantic model archive: ${archivePath}`);
console.info(`[SeekMind] semantic model sha256: ${sha256}`);
console.info(`[SeekMind] semantic model size: ${stat.size}`);
