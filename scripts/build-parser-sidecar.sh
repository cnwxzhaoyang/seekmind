#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 修复：旧 bash 构建入口降为兼容代理，避免与新的跨平台 Node 构建脚本继续分叉。
cd "$ROOT_DIR"
node scripts/build-parser-sidecar.mjs "$@"
