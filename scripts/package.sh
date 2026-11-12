#!/usr/bin/env bash
# Один Windows NSIS installer для установки и обновления. Ничего не публикует.
# Windows (Git Bash), корень репозитория: bash scripts/package.sh
set -euo pipefail

TARGET=x86_64-pc-windows-msvc
OUT=dist-release
VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
if [[ $# -gt 1 || ( $# -eq 1 && "$1" != "$VERSION" ) ]]; then
  echo "Версия должна совпадать с src-tauri/tauri.conf.json ($VERSION)" >&2
  exit 1
fi

node scripts/release-check.mjs --expect-version "$VERSION"

rm -rf "$OUT"
mkdir -p "$OUT" build-support
# Лицензии устанавливаются вместе с приложением, отдельных файлов рядом с setup.exe нет.
node scripts/release.mjs notices --out build-support/THIRD_PARTY_NOTICES.md

# Убираем старые установщики, чтобы в результат не попал exe от предыдущей сборки.
rm -f "target/$TARGET/release/bundle/nsis/"*-setup.exe
rm -f "target/$TARGET/release/yeskia-app.exe"
CARGO_TARGET_DIR="$PWD/target" npm run tauri build -- --ci --target "$TARGET" --bundles nsis --config scripts/installer.conf.json -- --locked --bin yeskia-app
node scripts/release-check.mjs --app "target/$TARGET/release/yeskia-app.exe"
INSTALLER="target/$TARGET/release/bundle/nsis/YESKIA_${VERSION}_x64-setup.exe"
test -s "$INSTALLER"
cp "$INSTALLER" "$OUT/"
echo "Готово: $OUT/$(basename "$INSTALLER")"
