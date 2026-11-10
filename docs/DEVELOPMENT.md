# Windows dev/build

Требуется: Windows 11 x64, [Rust stable](https://rustup.rs) (MSVC), Node.js 20+, WebView2 Runtime
(есть в Windows 11), Visual Studio Build Tools (C++).

```bash
npm install            # ставит ui/ и Tauri CLI
npm run tauri dev      # dev-режим (Vite + окно)
npm run tauri build    # release-сборка + NSIS-установщик (src-tauri/target/release/bundle)
```

Во время работы приложение не использует сеть, браузер и Docker.

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

## Smoke-test

`yeskia-app --smoke-test` инициализирует хранилище в `%APPDATA%\dev.yeskia.app`, собирает системные
данные и проводит цепочку connect → identify → disconnect на mock-устройстве; код возврата 0 при успехе.
Каталог можно перенаправить `YESKIA_DATA_DIR`.

## Данные пользователя

`%APPDATA%\dev.yeskia.app\yeskia.sqlite` (настройки, журнал) и `logs\yeskia.log` (без IMEI/серийников);
портативный режим (файл `yeskia.portable` рядом с exe) — подпапка `data`. Подробности: [DISTRIBUTION.md](DISTRIBUTION.md).
