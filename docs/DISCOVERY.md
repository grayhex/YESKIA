# Native Windows discovery и Doctor (issue #4)

Нативный слой вместо SymPort Bridge и браузерного relay: `crates/yeskia-discovery`.
**Read-only**: ничего не устанавливается, драйверы не подменяются (WinUSB/libusb не ставятся),
на устройство ничего не пишется, Nokia PC Suite продолжает работать.

## Что собирается

| Источник | API | Данные |
| --- | --- | --- |
| PnP-дерево | SetupAPI (`SetupDiGetClassDevsW`) + Configuration Manager (`CM_Get_Parent`, `CM_Get_DevNode_Status`) | instance id, цепочка родителей, класс, сервис, hardware id (VID/PID), код проблемы |
| COM | SetupAPI, класс `Ports`; имя `COMn` из «(COMn)» | порты потомков USB-устройства; **опционально** `CreateFileW` монопольно (занят/свободен), по умолчанию выключено |
| WPD/MTP | Windows Portable Devices (`IPortableDeviceManager`, `IPortableDevice` с `GENERIC_READ`) | производитель/модель/прошивка, число хранилищ и объектов верхнего уровня (имена файлов не читаются) |
| Тома | `GetLogicalDrives`/`GetDriveTypeW` | съёмные тома; связь с USB-родителем — через USBSTOR/DiskDrive в PnP-дереве |
| ОС | реестр `CurrentVersion` | сборка Windows |

Связь WPD ↔ USB-родитель определяется **по instance id** (путь WPD `\\?\usb#vid_…#…#{guid}` →
`USB\VID_…\…`, для составных устройств через интерфейс `&MI_xx` к родителю), а не по friendly name.
Несколько устройств оцениваются независимо (тест `multiple_devices_get_independent_wpd_association`).

## Состояния Doctor

`DoctorState`: `driver_missing` (CM_PROB 28/1) · `driver_error` (иной код) · `com_busy` · `com_present` ·
`wpd_only` · `mass_storage` · `unknown_mode`; глобально `no_usb_devices`. Отдельно `ProtocolStatus`:
`mtp_responding` — **аппаратный ответ**: WPD вернул контейнеры/объекты; `mtp_failed` (HRESULT);
`not_probed`. `pc_suite` и `pim` всегда `not_proven`: MTP/WPD не равно PC Suite и PIM. Модель не
выводится из VID/PID: `identity_confirmed = false`, `nokia_vendor_hint` (VID `0421`) — подсказка.
Не-Nokia MTP-устройства видны Doctor'у, но в список YESKIA не попадают.

## Использование

- UI: раздел «Диагностика» → «Запустить диагностику» / «Сохранить отчёт»
  (`%APPDATA%\YESKIA\reports\doctor-<ts>.json`).
- Без окна (чистый Windows 11, CI): `yeskia-app.exe --doctor [--com-busy] > report.json`
  (GUI-приложение не пишет в консоль — используйте перенаправление вывода).

## Privacy-safe отчёт

В отчёт попадают только: VID/PID, классы/сервисы/версии драйверов, коды проблем, счётчики,
WPD-модель/прошивка, версия/сборка ОС, коды ошибок этапов (HRESULT). **Не** попадают: IMEI, серийники,
PnP instance id, WPD device id, MAC, имена COM-портов, имена и содержимое файлов,
пользовательские имена устройств. Тест `report_is_privacy_safe` проверяет это на «заражённой»
топологии. Алиасы `device-1…` не связаны с серийниками.

## Интеграция с Device Core

`PnpTransport` (feature `engine`) реализует `DiscoveryProvider` + `DeviceTransport`:
- `list_devices` предлагает устройства с VID `0421`; имя нейтральное («USB 0421:0223 (WPD/MTP)»);
- id устройства — хеш instance id с ключом сессии (не раскрывает серийник);
- `connect` → read-only probe → матрица: `identity.read` (WPD-свойства) `available`/`error`;
  файлы, запись, PIM — `unsupported` (прототип только диагностический);
- отключение закрывает соединение; новое подключение — новая матрица; другой телефон — другой
  fingerprint (тесты `disconnect_removes_device_and_reconnect_does_not_inherit_matrix`,
  `a_different_phone_gets_a_different_id_and_fingerprint`);
- сбой перечисления одного транспорта не закрывает соединений и не скрывает другие транспорты.

## Статус и что осталось (приёмка #4)

| Пункт | Статус |
| --- | --- |
| Классификация состояний, WPD association, несколько устройств, COM busy, disconnect, не-Nokia MTP, privacy отчёта | `tested-on-mock` (поддельная PnP-топология) |
| Windows-сборщик (SetupAPI/CM/WPD) | `implemented`: компилируется под `windows` (проверка типов и clippy под `x86_64-pc-windows-gnu`), запускается на Windows-раннере CI без телефона (`--doctor`) |
| Запуск Doctor на **чистом Windows 11 x64** вручную | `not-tested` |
| Nokia `0421:0223`: проверка WPD association и read-only листинга на **физическом телефоне** | `not-tested` |
| Обезличенный отчёт на реальной Nokia (E52/E72), версии ОС/драйвера, итог `verified-on-device` | `not-tested` — **требует железа** |

### Как закрыть аппаратную часть

1. Подключить Nokia в нужном режиме USB, выполнить `yeskia-app.exe --doctor > report.json`
   (и при необходимости `--com-busy`), приложить `report.json` и версию Windows/драйвера к issue.
2. Повторить для каждого режима (см. матрицу), отключить кабель и убедиться, что устройство
   исчезло из списка и после повторного подключения матрица пустая (не унаследована).

### Матрица реальных режимов E52/E72

| Режим USB на телефоне | Ожидаемая сигнатура | Состояние Doctor | Статус |
| --- | --- | --- | --- |
| Media transfer (MTP), наблюдалось `0421:0223` (по описанию issue) | WPD без COM и буквы диска | `wpd_only` | `not-tested` |
| Mass storage | USBSTOR/DiskDrive, буква диска | `mass_storage` | `not-tested` |
| PC Suite / Ovi Suite | COM-порт/модем | `com_present` / `com_busy` | `not-tested` |
| Прочие режимы | — | — | `not-tested` |

Сигнатуры — гипотезы до прогона на устройстве; Doctor их не предполагает, а измеряет.
