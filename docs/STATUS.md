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
| Реальные устройства | `implemented` / `not-tested` | WPD/MTP read-only, Mass Storage и AT реализованы; физические проверки не выполнены. PC Suite unsupported. |
| Desktop UI: навигация, режимы, RU/EN, состояния (issue #2) | `tested-on-mock` | `ui/src/App.test.tsx` (vitest, UI-заглушка) |
| Рендер в установленном Tauri на Windows 11, DPI 100/150% | `implemented` | скриншоты в `docs/screenshots` сняты в Chromium; ручная проверка на Windows не выполнена |
| Матрица возможностей на соединении, evidence с fingerprint (issue #3) | `tested-on-mock` | `crates/yeskia-core/tests/engine.rs` |
| Очередь, отмена, таймаут (в т.ч. залипший драйвер), hot unplug, partial write, несовпадение fingerprint | `tested-on-mock` | там же |
| Безопасные пути, запрет silent overwrite, read-back SHA-256, `uncertain` | `tested-on-mock` / `tested-on-host` (пути, SQLite) | `paths::tests`, `engine.rs`, `storage::tests` |
| Реальные адаптеры | `implemented` / `not-tested` | WPD/MTP, Mass Storage и AT: см. строки ниже; PC Suite и OBEX unsupported. |
| Doctor: классификация PnP/WPD/COM, privacy-safe отчёт (issue #4) | `tested-on-mock` | `crates/yeskia-discovery/tests/doctor.rs` (поддельная топология) |
| Windows-сборщик SetupAPI/CM/WPD, read-only | `implemented` | компиляция под Windows + запуск `--doctor` на раннере CI (без телефона) |
| `PnpTransport` в Device Core: discovery, disconnect, fingerprint | `tested-on-mock` | `crates/yeskia-discovery/tests/adapter.rs` |
| WPD/MTP листинг и identity на реальной Nokia `0421:0223`, E52/E72 | `not-tested` | нужен физический телефон, см. docs/DISCOVERY.md |
| Иллюстрации телефонов по подтверждённой модели (перенос из s60modern) | `tested-on-mock` | `ui/src/phones/registry.test.ts`, `App.test.tsx`; права на изображения **не подтверждены**, см. docs/ASSETS.md |
| Файловый менеджер: скачивание/загрузка, коллизии, отмена, Unicode (issue #5) | `tested-on-mock` | `crates/yeskia-core/tests/transfer.rs`, `crates/yeskia-discovery/tests/files.rs`, `ui/src/FileManager.test.tsx` |
| Mass Storage: том ограничен устройством, read-back SHA-256 | `tested-on-host` | временные папки на реальной ФС; на физической карте `not-tested` |
| WPD/MTP read-only листинг/скачивание (Windows-бэкенд) | `implemented` | компиляция под Windows; на телефоне `not-tested`, запись не заявляется |
| Лаборатория NPC: формат отчёта v1, редактор приватных данных, анализатор журналов, scan (issue #6) | `tested-on-mock` | `crates/yeskia-lab/tests/lab.rs` (синтетические фикстуры, реплей-тесты) |
| Реальный обезличенный кейс «USB enumeration → identify attempt» | `not-tested` | нужен собственный телефон; инструкция в `docs/protocol/OBSERVATIONS.md` |
| Протоколы PC Suite (NPC) | `unsupported` | ни одна функция не заявляется без адаптера YESKIA, проверенного на телефоне |
| Файловые резервные копии: create/verify/restore-plan/apply только новых (issue #7) | `tested-on-host` / `tested-on-mock` | `crates/yeskia-core/tests/backup*.rs`, `crates/yeskia-discovery/tests/files.rs`, `ui/src/Backups.test.tsx` |
| Резервная копия с реального телефона/карты | `not-tested` | нужен физический телефон; это файловая копия, не полный backup (docs/BACKUPS.md) |
| Библиотека пакетов, инспектор SIS/SISX/JAR/JAD, подписи, наборы (issue #8) | `tested-on-host` | `crates/yeskia-core/tests/pkg.rs`, `packages.rs`; golden-сверка со SymPort; `ui/src/Packages.test.tsx` |
| Передача пакета на телефон (`packages.transfer`) | `tested-on-mock` | запись файла с read-back, не установка; на реальном телефоне/карте `not-tested` |
| Установка пакетов на телефоне, доверие сертификатам телефона | `not-tested` | инспектор не утверждает совместимость (`compatible` не выдаётся) |
| Рингтоны и подготовка медиа через внешний FFmpeg (issue #9) | `tested-on-host` | `crates/yeskia-core/tests/media.rs`, `media_jobs.rs`; без FFmpeg — `TOOL_MISSING` |
| Анализ и предпросмотр тем S60 (.skn/.mbm/.mif/SIS) | `tested-on-host` | `crates/yeskia-core/tests/theme.rs` (темы SymPort, golden) |
| Воспроизведение рингтона / применение темы / передача медиа на телефон | `not-tested` | нужен физический телефон; принятие темы прошивкой `unknown` |
| AT-канал по COM (3GPP, только чтение): идентификация, число контактов/сообщений (issue #10) | `tested-on-mock` | `crates/yeskia-core/tests/at.rs` (кадрирование, ошибки, hot-unplug), `crates/yeskia-discovery/tests/serial.rs`; синтетические диалоги, не снятые с телефона |
| AT-канал на реальном Nokia | `not-tested` | нужен собственный телефон; чек-лист в docs/protocol/AT_SERIAL.md |
| Любой сервис протокола Nokia PC Suite (NPC) | `unsupported` | нет реальных трасс (#6); ничего не заявляется |
| Сборка NSIS-установщика, портативного ZIP, SBOM, лицензий, контрольных сумм (issue #11) | `tested-on-host` | задача «Windows app build + smoke» в CI; не чистая Windows |
| Обновление из файла: размер/SHA-256/Ed25519, откладывание при записи на телефон | `tested-on-host` | `crates/yeskia-core/tests/update.rs`; ключ издателя не настроен — источник «неподписанный» |
| Первый запуск (Welcome → поиск → возможности → файлы) | `tested-on-mock` | `ui/src/Distribution.test.tsx` |
| Запуск на чистой Windows 11, деинсталлятор, SmartScreen, телефоны | `not-tested` | docs/release/MANUAL_CHECK.md |
