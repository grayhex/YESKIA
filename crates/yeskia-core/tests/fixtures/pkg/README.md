# Фикстуры пакетов (issue #8)

Три небольших файла скопированы из репозитория SymPort (`tests/fixtures`, лицензия MIT,
«SymPort contributors»). Они собраны 2026-11-03 реальным SDK **symbian-platform 0.3.0**
(Apache-2.0, <https://github.com/hpnkv/symbian-platform>) из исходников проекта (не из прошивок и не
из чужих приложений):

| Файл | Что это |
| --- | --- |
| `e32_probe_armv5t.exe` | собранный EXE (348 байт, UID3 0xE0000808, без импортов) |
| `sdk_probe_package.sis` | `symbian package` этого EXE (UID 0xE0000809, **без подписи**) |
| `sdk_probe_package_signed.sis` | то же с подписью одноразовым самоподписанным сертификатом «SymPort test only» (закрытый ключ не сохранён) |

Они проверяют, что разбор в YESKIA согласуется с независимой реализацией (SDK) и с поведением SymPort
(`golden/*.json` — вывод Python-реализации SymPort на тех же байтах, см. docs/PACKAGES.md).
Эти пакеты **не устанавливались** на телефон и не запускались в эмуляторе.
