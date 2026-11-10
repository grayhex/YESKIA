# Reuse matrix (s60modern → YESKIA)

> **Предварительная.** Составлена по описанию issue #1 без чтения кода
> [s60modern](https://github.com/grayhex/s60modern) (доступ к репозиторию из этой сессии не был выдан).
> Перед переносом любой функции её строку нужно сверить с исходником и лицензией файла.

Решения: **reuse** — взять как есть; **adapt** — перенести логику, переписав под Rust/локальное ядро;
**rewrite** — взять только идею; **drop** — не переносить.

| Область s60modern | Решение | Комментарий |
| --- | --- | --- |
| Дизайн Nokia Classic UI (SymPort) | adapt | issue #2; тема/компоненты в React, без серверных зависимостей |
| Модель возможностей и статусов (STATUS/LIMITATIONS) | adapt | реализована как `Capability/SupportStatus/Evidence` |
| Протокольные знания PC Suite / NPC | rewrite | issue #6/#10: реализация на Rust по реальным трассам |
| Разбор SIS/SISX/JAR, сертификаты | adapt | issue #8: переписано на Rust (`pkg/`), Python не нужен; сверено с исходником и golden-выводом SymPort, см. [PACKAGES.md](PACKAGES.md) |
| HTTP-каталог приложений, downloader | drop | сетевой, ссылки на сторонние дистрибутивы; офлайн-библиотека пользователя вместо него |
| Темы, рингтоны, подготовка медиа | adapt | issue #9: FFmpeg остаётся внешним процессом (как в SymPort), обёртка и анализ тем — Rust; см. [MEDIA.md](MEDIA.md). Сборка тем, Theme Studio, подпись — не переносились |
| Docker / Compose | drop | требование «без Docker» |
| Win Bridge relay | drop | прямой локальный доступ вместо relay |
| Серверная аутентификация, логин, аккаунты | drop | офлайн, одно рабочее место |
| API/веб-маршрутизация, HTTP-сервер | drop | команды Tauri вместо HTTP |
| Удалённое хранилище / NAS | drop | только локальный каталог пользователя |

## Лицензии компонентов (используются в этом репозитории)

| Компонент | Лицензия | Как используется |
| --- | --- | --- |
| flate2, sha1, x509-parser (+ der-parser, asn1-rs, oid-registry), zip | MIT OR Apache-2.0 / MIT | разбор пакетов |
| ring | Apache-2.0 AND ISC | проверка подписей |
| FFmpeg (внешний процесс, не линкуется и не поставляется) | LGPL/GPL по сборке пользователя | рингтоны, подготовка медиа; лицензия определяется и показывается |
| Tauri 2 (`tauri`, `tauri-build`, `@tauri-apps/*`) | MIT OR Apache-2.0 | оболочка |
| serde, serde_json, thiserror | MIT OR Apache-2.0 | ядро |
| rusqlite | MIT | ядро |
| SQLite (bundled) | Public Domain | хранилище |
| React, react-dom | MIT | UI |
| Vite, Vitest, @vitejs/plugin-react | MIT | сборка/тесты |
| Testing Library, jsdom | MIT | тесты |
| ESLint, typescript-eslint, Prettier, TypeScript | MIT / Apache-2.0 (TypeScript) | линтеры |

Лицензия самого YESKIA — MIT (`Cargo.toml`); файл `LICENSE` добавляется владельцем репозитория.

## Граница GPL (Gammu и подобные)

Gammu (GPL-2.0-or-later) и любой другой GPL-код **не линкуется** в `yeskia-core` и не
включается в бинарник. Допустим только как **отдельный, явно поставляемый внешний процесс**,
вызываемый через адаптер транспорта (stdin/stdout/файлы), с отдельным уведомлением о лицензии и
возможностью удалить без потери базовых функций. Исходный код GPL-компонентов нельзя копировать
в crates проекта. Решение по каждому такому компоненту фиксируется в этой матрице до интеграции.
