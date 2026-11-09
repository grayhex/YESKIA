# Архитектура YESKIA

YESKIA — офлайн-приложение для Windows 11 x64 для работы с реальными Nokia/Symbian.
Нет обязательных HTTP-сервера, браузера, Docker, NAS, Windows Bridge, логина или облака.

## Слои

```
React/TS (ui/)  ──Tauri commands (типизированные, JSON)──▶  src-tauri (тонкая оболочка)
                                                                 │
                                                                 ▼
                                             yeskia-core (чистый Rust, без UI и без HTTP)
   device · transport · operations · engine · storage · diagnostics · catalog · inspector · mock · app
                                                                 │
                                           Transport (trait) ◀── адаптеры: mock, WPD/MTP, Mass Storage, PC Suite…
```

- **WebView2 — только локальный UI** (`frontendDist`, CSP без внешних источников).
- **`yeskia-core`** не зависит от Tauri и тестируется на любой ОС; `src-tauri` лишь
  вызывает `AppCore`.
- API ядра не привязан к Web/HTTP: единственный вход для UI — `Engine` (через `AppCore`).

## Модель

| Тип | Назначение |
| --- | --- |
| `Device` | Устройство: `origin` (`mock`/`real`), `transport`, `connection`, `capabilities`. Серийник — `Sensitive` (маска в JSON/Debug). |
| `Connection` | `ConnectionState`: `disconnected → connecting → connected → disconnecting`; ведётся `Engine`. |
| `Transport` | Trait: `enumerate / connect / disconnect / is_present / execute`. Реальные USB/COM/WPD — отдельные реализации, UI о них не знает. |
| `Capability` | `id` + `SupportStatus` + `Evidence[]`. Заявленная модель телефона **не** подтверждает функцию. |
| `Evidence` | Что подтверждает статус: `mock_test`, `host_test`, `device_run`, `trace`. |
| `OperationResult` | `operation_id`, `outcome` (`success/failed/cancelled`), `data`, `error: ErrorInfo`, `evidence`, `origin`. |
| `ErrorCode`/`ErrorInfo` | Стабильные коды (`DEVICE_DISCONNECTED`, `OPERATION_FAILED`, …), `retryable`, текст без IMEI/серийников. |

## Жизненный цикл и гарантии `Engine`

1. На устройство — не более одной операции одновременно (`OPERATION_IN_PROGRESS`).
2. **Никаких автоповторов.** Ошибка записи возвращается как `Failed`; повтор — только ручной.
3. Транспорт обязан вызывать `OperationContext::checkpoint()` между шагами (отмена + присутствие
   устройства) и **фиксировать запись атомарно в конце**. Отключение кабеля → `DEVICE_DISCONNECTED`,
   соединение закрывается, запись не фиксируется (проверено тестом на mock).
4. `disconnect()` отменяет текущую операцию; `cancel(operation_id)` — отдельная отмена.
5. Каждая операция в журнале SQLite: `running → success/failed/cancelled`. После аварийного
   завершения `running` превращается в `interrupted` и **не** возобновляется.

## Хранилище и приватность

- SQLite (`rusqlite`, bundled) в `%APPDATA%\YESKIA\yeskia.sqlite` (`YESKIA_DATA_DIR` переопределяет);
  версия схемы — `PRAGMA user_version`; БД новее приложения не открывается.
- Журнал операций хранит необратимый отпечаток id устройства (`device_ref`), не сам id.
- `diagnostics::redact` применяется ко всем сообщениям журнала и ошибок: IMEI-подобные числа
  (14–16 цифр) и значения `serial=/sn:/imei=/imsi=` маскируются. Лог-файл — `logs\yeskia.log`.

## Mock-устройство

`MockTransport` — воспроизводимое устройство (`origin = mock`, имя с суффиксом `(MOCK)`, тег в UI,
evidence `mock_test`). Умеет `unplug()`, `fail_next_write()`, замедленную запись — для тестов
отмены/отключения. По умолчанию выключено (`Settings.use_mock_device = false`). Отдельно в
`ui/src/mockBackend.ts` есть **UI-заглушка** для браузера/vitest; в интерфейсе она показывается баннером.

## Границы (что НЕ делаем)

Docker/Compose, Win Bridge relay, серверная аутентификация, веб-маршрутизация, удалённое
хранилище; прошивальщик, обход защит, автоустановка драйверов.

## Python

Только опциональный, явно поставляемый sidecar для отдельных инструментов. Базовые функции
запускаются без Python.

## Сборка и запуск

См. [DEVELOPMENT.md](DEVELOPMENT.md).
