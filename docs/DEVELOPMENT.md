# Windows dev/build

Требуется: Windows 11 x64, [Rust stable](https://rustup.rs) (MSVC), Node.js 22, WebView2 Runtime
(есть в Windows 11), Visual Studio Build Tools (C++).

```bash
npm install            # ставит ui/ и Tauri CLI
npm run tauri dev      # dev-режим (Vite + окно)
bash scripts/package.sh # один NSIS-установщик в dist-release/
```

Данные телефона обрабатываются локально. Кнопка обновления открывает страницу релизов в браузере.

## Проверки (то же делает CI)

```bash
cargo fmt --all --check
cargo clippy -p yeskia-core --all-targets -- -D warnings
cargo test                      # ядро (default-members); работает на любой ОС
npm run ui:typecheck && npm run ui:lint && npm run ui:test
cargo run -p yeskia-app -- --smoke-test   # старт без окна, печатает JSON-отчёт
```

`cargo build/test` без `-p` собирают только `yeskia-core`; оболочка `yeskia-app` требует WebView
(Windows — WebView2).

## Необязательные интеграционные тесты FFmpeg

Обычный CI не устанавливает FFmpeg и не запускает девять тестов с настоящим
кодированием медиа. Они помечены `#[ignore]`; проверки ядра, тем, валидации путей
и UI продолжают выполняться. Для явного запуска с установленными `ffmpeg` и `ffprobe`:

```bash
YESKIA_REQUIRE_FFMPEG=1 cargo test -p yeskia-core --test media --test media_jobs -- --ignored
```

## Smoke-test

`yeskia-app --smoke-test` инициализирует хранилище в `%APPDATA%\dev.yeskia.app`, собирает системные
данные и проводит цепочку connect → identify → disconnect на mock-устройстве; код возврата 0 при успехе.
Каталог можно перенаправить `YESKIA_DATA_DIR`.

## Данные пользователя

`%APPDATA%\dev.yeskia.app\yeskia.sqlite` (настройки, журнал) и `logs\yeskia.log` (без IMEI/серийников);
портативный режим (файл `yeskia.portable` рядом с exe) — подпапка `data`. Подробности: [DISTRIBUTION.md](DISTRIBUTION.md).
