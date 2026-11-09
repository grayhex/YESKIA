# Статусы функциональности

Статус относится к **возможности**, а не к модели телефона. Заявленная модель ничего не подтверждает.

| Статус | Значение |
| --- | --- |
| `implemented` | Код есть, автоматических/аппаратных подтверждений нет. |
| `tested-on-mock` | Проверено автотестами на mock-устройстве. |
| `tested-on-host` | Проверено на хосте (реальные Windows API/файлы), без телефона. |
| `verified-on-device` | Подтверждено на реальном устройстве; нужна ссылка на evidence (трасса/отчёт). |
| `unsupported` | Не поддерживается (или не подтверждено, что работает). |

В коде: `SupportStatus` (`crates/yeskia-core/src/device.rs`).

## Текущее состояние (фундамент, issue #1)

| Возможность | Статус | Основание |
| --- | --- | --- |
| Модель устройств/операций/ошибок, `Engine` | `tested-on-mock` | `crates/yeskia-core/tests/engine.rs` |
| Отмена, отключение во время записи, отсутствие повтора записи | `tested-on-mock` | там же |
| SQLite-хранилище, журнал, `interrupted` после падения | `tested-on-host` | `storage::tests` (реальный файл SQLite) |
| Редактирование IMEI/серийников | `tested-on-host` | `diagnostics::tests` |
| Tauri-команды Rust → React | `implemented` | проверяется в CI на Windows (smoke-test + сборка); не запускалось на устройстве |
| Реальные устройства (WPD/MTP, Mass Storage, PC Suite) | `unsupported` | issue #4–#6 |
| Desktop UI: навигация, режимы, RU/EN, состояния (issue #2) | `tested-on-mock` | `ui/src/App.test.tsx` (vitest, UI-заглушка) |
| Рендер в установленном Tauri на Windows 11, DPI 100/150% | `implemented` | скриншоты в `docs/screenshots` сняты в Chromium; ручная проверка на Windows не выполнена |
| Матрица возможностей на соединении, evidence с fingerprint (issue #3) | `tested-on-mock` | `crates/yeskia-core/tests/engine.rs` |
| Очередь, отмена, таймаут (в т.ч. залипший драйвер), hot unplug, partial write, несовпадение fingerprint | `tested-on-mock` | там же |
| Безопасные пути, запрет silent overwrite, read-back SHA-256, `uncertain` | `tested-on-mock` / `tested-on-host` (пути, SQLite) | `paths::tests`, `engine.rs`, `storage::tests` |
| Реальные адаптеры (WPD/MTP, Mass Storage, PC Suite, OBEX) | `unsupported` | #4–#6, #10; контракт трейтов готов |
| Doctor: классификация PnP/WPD/COM, privacy-safe отчёт (issue #4) | `tested-on-mock` | `crates/yeskia-discovery/tests/doctor.rs` (поддельная топология) |
| Windows-сборщик SetupAPI/CM/WPD, read-only | `implemented` | компиляция под Windows + запуск `--doctor` на раннере CI (без телефона) |
| `PnpTransport` в Device Core: discovery, disconnect, fingerprint | `tested-on-mock` | `crates/yeskia-discovery/tests/adapter.rs` |
| WPD/MTP листинг и identity на реальной Nokia `0421:0223`, E52/E72 | `not-tested` | нужен физический телефон, см. docs/DISCOVERY.md |
