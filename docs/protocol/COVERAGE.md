# Матрица покрытия сервисов по моделям и прошивкам

Статусы: `verified-on-device` (есть обезличенный отчёт и ссылка), `implemented` (код есть, телефон не проверен),
`unsupported` (не реализовано/не заявлено), `not-tested`. Ячейка меняется на `verified-on-device` только записью
с ссылкой на отчёт из `docs/protocol/cases` (`evidence = adapter-verified`).

| Сервис | Канал | E52 | E72 | N95 | Symbian^3 (N8/E7) | Заметки |
| --- | --- | --- | --- | --- | --- | --- |
| Идентификация (`AT+CGMI/CGMM/CGMR`) | AT по COM (3GPP) | implemented / `not-tested` | implemented / `not-tested` | implemented / `not-tested` | implemented / `not-tested` | синтетические диалоги; режим PC Suite должен давать COM |
| Число контактов (`AT+CPBS?`) | AT по COM | implemented / `not-tested` | implemented / `not-tested` | implemented / `not-tested` | implemented / `not-tested` | только счётчик выбранного хранилища |
| Число сообщений (`AT+CPMS?`) | AT по COM | implemented / `not-tested` | implemented / `not-tested` | implemented / `not-tested` | implemented / `not-tested` | только счётчик |
| Список контактов | AT по COM | unsupported | unsupported | unsupported | unsupported | следующая задача, после проверки на телефоне |
| Контакты, календарь / задачи | USB OBEX/PCCS | implemented / not-tested | implemented / not-tested | implemented / not-tested | implemented / not-tested | Rust readers, synthetic FakePhone; физическая приёмка отсутствует |
| Чтение SMS | COM/PDU | implemented / not-tested | implemented / not-tested | implemented / not-tested | implemented / not-tested | native PDU reader, без отправки/удаления |
| Независимо подтверждённый аппаратный NPC-сервис | NPC | not-tested | not-tested | not-tested | not-tested | Код PCCS reader не равен real trace/device evidence, #6/#10 |
| Запись контактов, отправка SMS, удаление, синхронизация | любой | unsupported | unsupported | unsupported | unsupported | только отдельными задачами |
| Файлы (запись) | AT/NPC | unsupported | unsupported | unsupported | unsupported | файлы — Mass Storage/WPD (см. STATUS.md) |

Прошивки: ни одна версия прошивки не проверялась — столбца «прошивка» нет, пока нет ни одного реального отчёта.

## Independent ADBian agent path

Windows Bluetooth RFCOMM/SPP + upstream ADBian framing is a separate adapter; it is not
Nokia USB NPC, USB COM, WPD or PCCS implementation evidence. Golden replies and fake
transfers in `crates/yeskia-core/tests/adbian.rs` / `crates/yeskia-adbian/tests/adapter.rs`
are synthetic. No physical E72 feature has been promoted from them.

The optional YESKIA helper adds a versioned AppArc registry protocol source, with PIM
flags false. [Research plans](../agent/research/README.md) cover filesystem, system APIs
and host/phone correlation; a real PC Suite comparison is still outstanding. See the
[acceptance ledger](../agent/ACCEPTANCE.md) before claiming hardware/protocol coverage.
