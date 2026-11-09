# YESKIA

Офлайн-приложение для Windows 11 x64 для работы с реальными Nokia/Symbian.
Стек: **Tauri 2 + Rust + React/TypeScript**. Без сервера, Docker, облака и логина.

- Архитектура: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Статусы функций: [docs/STATUS.md](docs/STATUS.md)
- Reuse из s60modern и лицензии: [docs/REUSE_MATRIX.md](docs/REUSE_MATRIX.md)
- Device Core (матрица, очередь, запись): [docs/DEVICE_CORE.md](docs/DEVICE_CORE.md)
- Windows discovery и Doctor: [docs/DISCOVERY.md](docs/DISCOVERY.md)
- Интерфейс и скриншоты: [docs/UI.md](docs/UI.md)
- Сборка на Windows: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)

Структура: `crates/yeskia-core` (ядро), `crates/yeskia-discovery` (Windows PnP/WPD/Doctor), `src-tauri` (оболочка), `ui` (React).
