# Аудит всех issues — 12 ноября 2026

База проверки: `main` после PR #40, #44, #45 и #46, исходные требования всех
22 issues, код/контрактные тесты, документы и [Windows CI #46](https://github.com/grayhex/YESKIA/actions/runs/43161511435).
Этот аудит готовит 0.1.1. Закрытый PR не означает автоматически выполненную
аппаратную приёмку. Синтетический агент не заменяет E72.

Обозначения: **готово** — программный объём завершён; **частично** — есть код,
но остались требования; **аппаратная приёмка** — нужен реальный стенд владельца.
Шесть ранее закрытых задач возвращаются в open: их собственные комментарии и
приёмка явно оставляли работу. Закрывается #41 после завершения экспорта отчёта
и успешной сборки кандидата; физический TLS остаётся not-tested, как допускает issue.

| Issue | Решение по статусу | Сделано / что осталось |
| --- | --- | --- |
| [#1](https://github.com/grayhex/YESKIA/issues/1) | Оставить closed: фундамент готов | Tauri/Rust/SQLite/typed IPC/CI/privacy/reuse. Отдельная чистая Windows и телефон — #11, а не результат mock smoke. |
| [#2](https://github.com/grayhex/YESKIA/issues/2) | Оставить closed: исходный UI заменён согласованным #37 | Fluent, RU/EN, keyboard, темы и локальный логотип. Реальный Windows DPI не выдаётся за browser emulation. |
| [#3](https://github.com/grayhex/YESKIA/issues/3) | Оставить closed: контракт готов | Queues, capability/evidence, fingerprint, cancellation, hot-unplug, partial write и no replay покрыты тестами. |
| [#4](https://github.com/grayhex/YESKIA/issues/4) | Вернуть open: аппаратная приёмка | Doctor/PnP/WPD реализованы. Исторический отчёт E72: 2 хранилища/38 объектов; после исправлений нужен новый отчёт, disconnect/reconnect и чистая Windows. |
| [#5](https://github.com/grayhex/YESKIA/issues/5) | Вернуть open: аппаратная приёмка | Файл-менеджер и host/mock проверки готовы; нет текущего E72 download/hash и real Mass Storage copy/read-back. |
| [#6](https://github.com/grayhex/YESKIA/issues/6) | Вернуть open: нет реального capture case | Runbook, privacy scanner, схемы и synthetic fixtures готовы. Не получен аппаратный enumerate → identify attempt с обезличенной трассой. |
| [#7](https://github.com/grayhex/YESKIA/issues/7) | Оставить closed: локальный backup готов | Create/verify/plan/apply новых файлов, race/conflicts/cancel/manifest checks. Не системный и не PIM backup. |
| [#8](https://github.com/grayhex/YESKIA/issues/8) | Оставить closed: офлайн-инспектор готов | Library, SIS/JAR/certificate inspection и golden tests. Неизвестное не становится compatible; доставка не означает установку. |
| [#9](https://github.com/grayhex/YESKIA/issues/9) | Вернуть open: неполный исходный объём | Рингтоны/пресеты/preview тем готовы; нет редактирования палитры/ресурсов, сборки и подписи темы. FFmpeg внешний. |
| [#10](https://github.com/grayhex/YESKIA/issues/10) | Вернуть open: независимая device-проверка | Нативные AT, OBEX/PCCS и PDU readers есть и проверены синтетически; нет подтверждённого текущим адаптером реального NPC/PIM сервиса. |
| [#11](https://github.com/grayhex/YESKIA/issues/11) | Вернуть open: ручная приёмка релиза | Настоящий setup.exe/install/reinstall/UPDATE проверяются Windows CI. Чистая Windows, выбор удаления данных, SmartScreen, upgrade между выпущенными версиями и телефон ещё не приняты. Пустой prerelease 0.1.0 не считается поставленным EXE. |
| [#30](https://github.com/grayhex/YESKIA/issues/30) | Оставить open: SIS + E72 | Пины/скрипты/ARM Rust check/инструкции готовы; отсутствует полная Nokia SDK/C++/SIS сборка трёх пакетов и install/reboot на E72. |
| [#31](https://github.com/grayhex/YESKIA/issues/31) | Оставить open: Windows/E72 | Native RFCOMM/SDP/framing/Phone-class filter готовы; реальный канал, ping/ls/stat/cat и радио-поведение не проверены. |
| [#32](https://github.com/grayhex/YESKIA/issues/32) | Оставить open: интеграция с реальным агентом | Core/FileProvider/UI/Sideload готовы. Добавлен synthetic C:/E:/Z: + RO + Bluetooth failure/USB isolation test; физические корни, права и coexistence ещё не подтверждены. |
| [#33](https://github.com/grayhex/YESKIA/issues/33) | Оставить open: Lab на E72 | Arming, shared queue, реальные upstream verbs, explicit confirm и privacy готовы. Нет аппаратных system/exec/destructive cases и optional reset case. |
| [#34](https://github.com/grayhex/YESKIA/issues/34) | Оставить open: agent-side реализация | Versioned helper/AppArc source/fake tests есть; нет SIS/loader/E72 memory proof. Contacts/calendar/tasks/SMS exports не реализованы. |
| [#35](https://github.com/grayhex/YESKIA/issues/35) | Оставить open: lifecycle на E72 | Wizard, staging/consent/manual recovery готовы. Нет physical launch/relaunch/update/reboot/autostart/version evidence. |
| [#36](https://github.com/grayhex/YESKIA/issues/36) | Оставить open: реальные research cases | Три плана и privacy правила есть; отсутствуют три аппаратных отчёта и сравнение с реальной NPC-сессией. |
| [#37](https://github.com/grayhex/YESKIA/issues/37) | Оставить closed: Fluent выполнен | React-компоненты, все разделы, темы, RU/EN, keyboard, mock/empty, 343 browser checks и текущие screenshots. Старые прототипы/снимки удалены, новый логотип — по прямому уточнению владельца. |
| [#41](https://github.com/grayhex/YESKIA/issues/41) | Закрыть completed: host/manual scope готов | Пины v1.4.4/v21 реально сверены; bounded SIS/ZIP/cache/staging, ручной apply/restore, stack exclusions, часы, прогресс и privacy report. При отсутствии проверенных системных прав остаётся предусмотренный issue ручной путь. HTTPS/recovery на E72 остаются not-tested и входят в #43/#36. |
| [#42](https://github.com/grayhex/YESKIA/issues/42) | Оставить open: writable provider и UX остатки | Локальная книга/импорт/экспорт/merge/undo и read snapshots готовы. Нет Create/Update/Delete на телефоне и аппаратного read-back; sync dry-run/conflict flow, большой dataset/полные shortcuts требуют завершения. |
| [#43](https://github.com/grayhex/YESKIA/issues/43) | Оставить open: полный first-run acceptance | Typed scoped progress/deep links/manual flags готовы. Нужны явные вызовы/SMS manual cases, полный COM busy/unplug/second phone E2E и прохождение на RM-530 054.005. |

## Следующие реальные блокеры

1. Windows 11 + собственная E72: USB/BT baseline, текущая прошивка/драйверы,
   подключение только выбранного телефона, native read/hash, reconnect и privacy report.
2. Nokia SDK / EPOC pipeline: полностью собрать SIS и установить, отдельно проверить
   daemon/panel/autostart. Host Rust check не выдаётся за SIS build.
3. Writable PIM: независимые protocol evidence, затем тестовые create/update/read-back,
   и только после этого ограниченные bulk/delete на тестовых контактах.
4. Темы: отдельный редактор/сборка, а не выдача существующего preview за редактор.
5. Релиз 0.1.1: Windows x64 GUI PE + install payload/hash/runtime checks; ручная
   проверка чистой Windows и межверсионного обновления. Ничего автоматически не публикуется.

## Очистка

Удалены 43 устаревших файла (~4.7 MB): Soft UI CSS, завершённые HTML/design задания,
старые Classic/Soft UI/Fluent screenshots. Сохранены текущие connection light/dark
снимки и актуальный layout report. **Не удалены** SIS/EXE/темы в `tests/fixtures`:
они являются входами golden-тестов, а не остатками сборки. Не удалены SDK pins,
исследовательские планы и лицензии. GitHub workflow artifacts и опубликованные
релизы не удалялись: запрос касался файлов репозитория.
