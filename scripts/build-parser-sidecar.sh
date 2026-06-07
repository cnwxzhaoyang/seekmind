#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESOURCE_DIR="$ROOT_DIR/src-tauri/resources"
APP_RESOURCE_DIR="$ROOT_DIR/src-tauri/app-resources"
BUILD_DIR="$ROOT_DIR/.docmind-build/parser-sidecar"
DIST_DIR="$BUILD_DIR/dist"
WORK_DIR="$BUILD_DIR/build"
SPEC_DIR="$BUILD_DIR/spec"
SIDE_CAR_BASE="docmind-parser"
OCR_BASE="ocr"
TARGET_TRIPLE="$(rustc -vV | awk -F': ' '/host: / {print $2}')"
OUTPUT_NAME="${SIDE_CAR_BASE}-${TARGET_TRIPLE}"
OCR_DIR="$APP_RESOURCE_DIR/$OCR_BASE"
VISION_OCR_BIN_NAME="docmind-vision-ocr"
VISION_OCR_BIN_DEST="$OCR_DIR/vision-ocr"

mkdir -p "$RESOURCE_DIR" "$APP_RESOURCE_DIR" "$DIST_DIR" "$WORK_DIR" "$SPEC_DIR"

build_vision_ocr_helper() {
  echo "Building bundled Vision OCR helper..."
  # 修复：先清理旧 OCR 资源目录，避免 Cargo/Tauri 在编译阶段扫描到残留的 tessdata 坏链接。
  rm -rf "$OCR_DIR"

  cargo build \
    --manifest-path "$ROOT_DIR/src-tauri/Cargo.toml" \
    --release \
    --bin "$VISION_OCR_BIN_NAME"

  local helper_bin="$ROOT_DIR/src-tauri/target/release/$VISION_OCR_BIN_NAME"
  if [ ! -x "$helper_bin" ]; then
    echo "Vision OCR helper build failed: $helper_bin not found"
    return 1
  fi

  mkdir -p "$OCR_DIR"
  cp "$helper_bin" "$VISION_OCR_BIN_DEST"
  chmod +x "$VISION_OCR_BIN_DEST"

  echo "Bundled Vision OCR helper: $VISION_OCR_BIN_DEST"
}

build_vision_ocr_helper

python3 -m pip install -r "$ROOT_DIR/parser/requirements.txt"

if ! python3 -m PyInstaller --version >/dev/null 2>&1; then
  python3 -m pip install pyinstaller
fi

python3 -m PyInstaller \
  --noconfirm \
  --clean \
  --onedir \
  --name "$SIDE_CAR_BASE" \
  --distpath "$DIST_DIR" \
  --workpath "$WORK_DIR" \
  --specpath "$SPEC_DIR" \
  "$ROOT_DIR/parser/docmind_parser/__main__.py"

rm -rf "$APP_RESOURCE_DIR/$OUTPUT_NAME"
cp -R "$DIST_DIR/$SIDE_CAR_BASE" "$APP_RESOURCE_DIR/$OUTPUT_NAME"
chmod +x "$APP_RESOURCE_DIR/$OUTPUT_NAME/$SIDE_CAR_BASE"

if [ -d "$ROOT_DIR/.docmind-cache/fastembed" ]; then
  rm -rf "$APP_RESOURCE_DIR/fastembed"
  rm -f "$APP_RESOURCE_DIR/fastembed-cache.tar.gz"
  tar -czf "$APP_RESOURCE_DIR/fastembed-cache.tar.gz" -C "$ROOT_DIR/.docmind-cache/fastembed" .
  echo "Bundled FastEmbed cache archive: $APP_RESOURCE_DIR/fastembed-cache.tar.gz"
else
  echo "FastEmbed cache not found at $ROOT_DIR/.docmind-cache/fastembed; semantic model may need runtime download"
fi

echo "Built parser sidecar: $APP_RESOURCE_DIR/$OUTPUT_NAME"
