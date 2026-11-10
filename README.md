# YESKIA

Офлайн-приложение для Windows 11 x64 для работы с реальными Nokia/Symbian.
Стек: **Tauri 2 + Rust + React/TypeScript**. Без сервера, Docker, облака и логина.

- Архитектура: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Статусы функций: [docs/STATUS.md](docs/STATUS.md)
- Reuse из s60modern и лицензии: [docs/REUSE_MATRIX.md](docs/REUSE_MATRIX.md)
- Device Core (матрица, очередь, запись): [docs/DEVICE_CORE.md](docs/DEVICE_CORE.md)
- Windows discovery и Doctor: [docs/DISCOVERY.md](docs/DISCOVERY.md)
- Лаборатория PC Suite (регламент захвата, отчёты): [docs/protocol/NPC_LAB.md](docs/protocol/NPC_LAB.md)
- Файловый менеджер: [docs/FILE_MANAGER.md](docs/FILE_MANAGER.md)
- Пакеты, библиотека и инспектор: [docs/PACKAGES.md](docs/PACKAGES.md)
- Резервные копии файлов: [docs/BACKUPS.md](docs/BACKUPS.md)
- Графика телефонов, происхождение и права: [docs/ASSETS.md](docs/ASSETS.md)
- Интерфейс и скриншоты: [docs/UI.md](docs/UI.md)
- Сборка на Windows: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)

Структура: `crates/yeskia-core` (ядро), `crates/yeskia-discovery` (Windows PnP/WPD/Doctor), `crates/yeskia-lab` (лаборатория совместимости), `src-tauri` (оболочка), `ui` (React).
