#!/usr/bin/env bash
# Сборка Windows-артефактов: NSIS-установщик, портативный ZIP, уведомления о лицензиях, SBOM,
# контрольные суммы, манифест обновления и заметки. Ничего не публикует и не подписывает.
# Запуск на Windows (Git Bash) из корня репозитория:  bash scripts/package.sh [версия]
set -euo pipefail

OUT=dist-release
VERSION="${1:-$(node -p "require('./src-tauri/tauri.conf.json').version")}"
rm -rf "$OUT" && mkdir -p "$OUT/portable/YESKIA"

# 1. Установщик (NSIS, установка для текущего пользователя, без прав администратора)
npm run tauri build -- --bundles nsis
INSTALLER=$(ls target/release/bundle/nsis/*-setup.exe | head -n 1)
cp "$INSTALLER" "$OUT/"
INSTALLER_NAME=$(basename "$INSTALLER")

# 2. Портативная сборка: тот же exe + маркер yeskia.portable (данные в подпапке data)
cp target/release/yeskia-app.exe "$OUT/portable/YESKIA/YESKIA.exe"
: > "$OUT/portable/YESKIA/yeskia.portable"
cat > "$OUT/portable/YESKIA/README.txt" <<'TXT'
YESKIA — портативная сборка.

* Запустите YESKIA.exe. Файл yeskia.portable рядом с программой включает портативный режим:
  настройки, журнал и резервные копии лежат в подпапке data; профиль Windows и реестр не используются.
* Удаление: удалите папку. Чтобы сохранить данные, сначала скопируйте папку data.
* Нужен Microsoft Edge WebView2 Runtime (входит в Windows 11). Приложение ничего не скачивает и не обновляется само.
* Сборка не подписана издателем: Windows SmartScreen может предупредить о неизвестном издателе.
TXT
cp docs/release/BETA_TESTING.md "$OUT/portable/YESKIA/BETA_TESTING.md"
PORTABLE_NAME="YESKIA_${VERSION}_x64-portable.zip"
powershell.exe -NoProfile -Command "Compress-Archive -Path '$OUT/portable/YESKIA' -DestinationPath '$OUT/$PORTABLE_NAME' -Force"
rm -rf "$OUT/portable"

cp docs/release/BETA_TESTING.md "$OUT/BETA_TESTING.md"

# 3. Уведомления, SBOM, заметки, манифест обновления, контрольные суммы
node scripts/release.mjs notices --out "$OUT/THIRD_PARTY_NOTICES.md"
node scripts/release.mjs sbom --out "$OUT/sbom.cdx.json"
node scripts/release.mjs notes --version "$VERSION" --out "$OUT/RELEASE_NOTES.md"
node scripts/release.mjs manifest --installer "$OUT/$INSTALLER_NAME" --version "$VERSION" --out "$OUT/update.json"
node scripts/release.mjs checksums --out "$OUT/SHA256SUMS.txt" \
  "$OUT/$INSTALLER_NAME" "$OUT/$PORTABLE_NAME" "$OUT/THIRD_PARTY_NOTICES.md" "$OUT/sbom.cdx.json" \
  "$OUT/RELEASE_NOTES.md" "$OUT/update.json" "$OUT/BETA_TESTING.md"
echo "Готово: $OUT"; ls -l "$OUT"
