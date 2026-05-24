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
TARGET_TRIPLE="$(rustc -vV | awk -F': ' '/host: / {print $2}')"
OUTPUT_NAME="${SIDE_CAR_BASE}-${TARGET_TRIPLE}"

mkdir -p "$RESOURCE_DIR" "$APP_RESOURCE_DIR" "$DIST_DIR" "$WORK_DIR" "$SPEC_DIR"

if ! python3 -m pip show fastembed >/dev/null 2>&1; then
  python3 -m pip install -r "$ROOT_DIR/parser/requirements.txt"
fi

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

echo "Built parser sidecar: $APP_RESOURCE_DIR/$OUTPUT_NAME"
