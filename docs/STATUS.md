# Статус YESKIA — 12 ноября 2026

`implemented` — код есть; `tested-on-mock` — синтетические устройства/пакеты;
`tested-on-host` — реальная ОС, файловая система или установленный EXE;
`verified-on-device` — только обезличенная аппаратная проверка конкретного телефона
и прошивки. Успех Windows CI не становится `verified-on-device`.

Подтверждённая сборка **0.5.0**: [CI PR #47](https://github.com/grayhex/YESKIA/actions/runs/43169673961), включая полный installed/build SHA-256, runtime version/architecture и NSIS install/reinstall/UPDATE. [Аудит issues](ISSUE_AUDIT.md)
содержит остатки работы и причины незакрытых задач.

| Возможность | Статус | Evidence / ограничения |
| --- | --- | --- |
| Tauri/Rust/React, локальные настройки/SQLite, очереди, отмена, privacy | tested-on-host / tested-on-mock | Rust/UI/Windows CI, installed EXE smoke, контракты hot-unplug/timeout/uncertain writes |
| Fluent UI, RU/EN, явная light/dark тема, логотип, DPI | tested-on-host | 343 браузерные проверки в #46; браузерные mock-снимки не являются Windows/E72 hardware run |
| USB PnP/WPD/COM discovery и Doctor | tested-on-host / tested-on-mock | Windows API smoke и synthetic topology. Ранее пользователь сообщил E72 WPD association и listing 2 хранилищ/38 объектов; повторная приёмка текущего адаптера остаётся #4 |
| WPD/MTP download, Mass Storage copy/read-back | tested-on-mock / tested-on-host | Fake WPD и реальные host volumes; проверка на карте/телефоне #5 не завершена |
| Classic Bluetooth Phone class, SDP/RFCOMM | implemented; hardware not-tested | Пассивный paired cache, запрет audio/unknown class, открытие только по запросу. Native Windows build и contract tests, реальная E72 #31 |
| ADBian files/Lab/sideload, разделение маршрутов | tested-on-mock | C:/E:/Z: probes, Z: read-only, большие чтения, put/read-back, отдельные USB/Bluetooth очереди. Физическая приёмка #32/#33 остаётся |
| EPOC helper / собственный SIS-агент | implemented source; SIS/hardware not-tested | ARM Rust check, AppArc bridge, fake protocol. Полная SDK/C++/SIS сборка и E72 install/reboot — #30/#34/#35 |
| Agent-side contacts/calendar/tasks/SMS | unsupported | API audit и планы не являются экспортом PIM; #34 |
| USB OBEX/PCCS contacts/calendar/tasks reader | tested-on-mock | Rust-native framing и FakePhone fixtures; физическое чтение текущим адаптером #10/#42 |
| COM/PDU SMS reader и AT identity/counts | tested-on-mock | Синтетические диалоги, PDU validation; нет отправки/удаления SMS |
| Локальная книга контактов | tested-on-host / tested-on-mock | VCF/CSV/JSON, редактор, multivalue, merge preview, revision/undo, экспорт; writable Nokia provider отсутствует — #42 остаётся открытым |
| TLS Revival | tested-on-host / tested-on-mock; E72 not-tested | Проверены официальные пины и SIS/ZIP, staged SIS/read-back, ручные состояния/инструкции, обезличенный отчёт. Автоматические privileged apply/rollback заблокированы без доказательств; #41 |
| Подготовка E72 | tested-on-mock | Persistent scoped progress, реальные deep links и native proof hooks. Полный путь на RM-530 054.005, вызовы/SMS и аппаратная приёмка — #43 |
| Файловый backup / verify / restore-plan | tested-on-host | Local filesystem integration; не системный/PIM backup. Телефон — только по capability/evidence |
| Библиотека пакетов/инспектор/сертификаты | tested-on-host / tested-on-mock | Разрешённые golden fixtures, не обещает совместимость неизвестной прошивки. Передача не означает установку/доверие |
| FFmpeg/media и просмотр тем | tested-on-host для ранее выполненных FFmpeg runs; обычный CI без FFmpeg | FFmpeg внешний; 9 real codec tests opt-in. Редактор палитры/сборка темы не реализованы — #9 |
| NSIS EXE, установка/переустановка/UPDATE | tested-on-host | Windows runner проверяет installed app и данные. Чистая Windows, интерактивное удаление, SmartScreen и переход между выпущенными версиями — #11 |
| Независимая аппаратная NPC-сессия / research cases | not-tested | Нет обезличенных реальных трасс/трёх cases, #6/#10/#36 |

Полный список оставшихся device-проверок: [агент](agent/ACCEPTANCE.md),
[протоколы](protocol/COVERAGE.md), [ручной релизный чек-лист](release/MANUAL_CHECK.md).
