# Device Core (issue #3)

Единая точка входа для диагностики, чтения, копирования и позднее PIM: `Engine`
(`crates/yeskia-core/src/engine.rs`). Это перенос **управления операциями**, а не HTTP-слоя:
веб-эндпоинты и сервер не переносятся.

> Доступа к `s60modern` (`TRANSPORTS.md`, `symport/phone/*`) в сессии разработки не было:
> контракт реализован по описанию issue. Сверка семантики оригинального `DeviceManager`
> остаётся задачей ревью.

## Traits (`transport.rs`)

| Trait | Назначение |
| --- | --- |
| `DiscoveryProvider` | `discover()` — перечисление устройств. Успех **не** означает доступность операций. |
| `DeviceTransport: DiscoveryProvider` | `connect/disconnect/is_present`, `maturity(op)` (зрелость кода адаптера), `probe()` (**только чтение**), `identity()`, доступ к `files()` / `pim()`. |
| `FileProvider` | `list/stat/read/write` с `OperationContext` (отмена, таймаут, прогресс). |
| `PimProvider` | Счётчики контактов/SMS (по умолчанию «не поддерживается»); данные не попадают в журнал. |

WPD/MTP, Mass Storage, Serial/PC Suite и Bluetooth/OBEX — независимые реализации (#4–#6, #10).
Сейчас есть `MockTransport` (`mock.rs`), в тестах — «реальный» адаптер-обёртка.

## Матрица на конкретном соединении

`Engine::connect` = открыть транспорт → read-only `probe` → `CapabilityMatrix` для этого соединения.
Операции: `identity.read`, `files.list/read/write`, `contacts.read/write`, `sms.read/send`,
`packages.install`. Для каждой:

- **Availability**: `available` / `not-tested` / `unsupported` / `error` (+ `detail`).
- **Maturity** (отдельно): `unsupported` / `implemented` / `tested-on-mock` / `tested-on-host` /
  `verified-on-device`. Успешная операция на реальном устройстве повышает до `verified-on-device`
  только для этого соединения; на mock выше `tested-on-mock` не поднимается.
- **Evidence**: `kind`, `source` (`probe` / `operation:<id>`), `checked_at_ms`, `device_fingerprint`.

Правила: модель телефона и USB-перечисление матрицу не определяют; probe не пишет, поэтому
`files.write` остаётся `not-tested` до явной операции; ошибка операции переводит запись в `error`;
`unsupported` отклоняется на постановке в очередь; отправка SIS через `files.write` не меняет
`packages.install` (отдельная операция, пока `unsupported`).

## Очередь, отмена, таймауты, hot unplug

- На устройство — FIFO-очередь (`max_queue`, по умолчанию 8; сверх — `QUEUE_FULL`).
  Операция живёт как `submit → OperationHandle → wait`; `Engine::cancel(id)` работает и для
  ожидающих (они не доходят до устройства), и для выполняющихся.
- Операция выполняется в воркере, **сторож** следит за отменой, таймаутом (по типу операции) и
  `is_present`. «Залипший» драйвер не блокирует UI: после `cancel_grace` вернётся результат
  (`TIMEOUT`/`DEVICE_DISCONNECTED`), а слот очереди освободится только когда воркер реально выйдет —
  операции не перекрываются.
- `Engine::progress(op_id)` — `{done,total}` для UI; `disconnect()` отменяет всё, `list_devices()`
  замечает пропавшие устройства.
- **Fingerprint** = SHA-256(соль установки | транспорт | id | модель | серийник), 8 байт hex. Соль
  случайная, хранится в SQLite → серийник/IMEI перебором не восстановить. Проверяется перед
  операцией и после записи; несовпадение → `FINGERPRINT_MISMATCH`, соединение закрывается.

## Запись

1. Пути проверяются **до очереди** (`paths.rs`): диск `E:/…` или `/…`; запрещены `..`, `.`, пустые
   сегменты, `\`, управляющие символы, `<>:"|?*`, имена с точкой/пробелом на конце → `UNSAFE_PATH`.
2. Silent overwrite запрещён: по умолчанию `OverwritePolicy::Never` → `ALREADY_EXISTS`;
   замена — только явным `Replace`.
3. После записи — обратное чтение и сравнение SHA-256 (`VerificationFailed`, если не совпало).
4. **Автоповтора нет.** Любой сбой/отмена/таймаут/отключение во время записи → `uncertain = true`,
   `action = verify_device_contents`; повтор — только вручную по решению пользователя.
   Прерванная аварийным завершением запись помечается `interrupted` + `uncertain` в журнале.
5. Probe и автотесты не пишут на устройство; кнопка «Тестовая запись» в UI доступна только для
   mock-устройства.

## Ошибки и журнал

`ErrorInfo { code, message, retryable, action }`, где `action` — `reconnect`, `check_cable`,
`choose_another_path`, `wait_and_retry`, `retry_manually`, `verify_device_contents`; UI локализует.
Журнал SQLite (схема v2) хранит операцию, итог, код ошибки и `uncertain`; лог и журнал не содержат
путей, содержимого, контактов/SMS, IMEI/серийников и исходного id устройства.

## Tauri-контракт

`start_operation(id, request) → operation_id`, `await_operation(operation_id) → OperationResult`,
`operation_progress`, `cancel_operation`. Один и тот же типизированный ответ для mock и реального
транспорта (контрактный тест `ui_gets_the_same_typed_response_from_mock_and_real_transport`).
