import { createContext, useContext, type ReactNode } from "react";

export type Lang = "ru" | "en";

const ru = {
  appName: "YESKIA",
  appTagline: "Клиент для Nokia / Symbian",
  skipToContent: "Перейти к содержимому",
  navLabel: "Основная навигация",
  "nav.home": "Главная",
  "nav.connection": "Подключение",
  "nav.journal": "Журнал",
  "nav.settings": "Настройки",
  "mode.user": "Пользователь",
  "mode.developer": "Разработчик",
  "mode.label": "Режим",
  "lang.label": "Язык",
  "status.devices": "Устройств: {n}",
  "status.offline": "Работает офлайн",
  "status.noDevice": "Нет подключённого устройства",
  "status.connected": "Подключено: {name}",
  "status.mockOn": "Включено демо-устройство",
  "status.uiMock": "UI-заглушка",
  "banner.uiMock":
    "UI-заглушка: данные не от Rust-ядра (запуск вне Tauri). Цифры не относятся к реальным устройствам.",
  "tag.mock": "MOCK",
  "tag.real": "Реальное",
  "conn.disconnected": "Отключено",
  "conn.connected": "Подключено",
  "conn.connecting": "Подключение…",
  "conn.disconnecting": "Отключение…",
  "home.title": "Главная",
  "home.noDeviceTitle": "Устройство не выбрано",
  "home.noDeviceText": "Подключите телефон по USB или включите демо-устройство в настройках.",
  "home.goConnection": "Открыть раздел «Подключение»",
  "home.metrics": "Показатели",
  "home.metric.devices": "Найдено устройств",
  "home.metric.connection": "Соединение",
  "home.metric.capabilities": "Возможности",
  "home.metric.ops": "Операции за сессию",
  "home.metric.opsValue": "{ok} успешно · {bad} с ошибкой",
  "home.metric.capsValue": "{n} доступно из {total}",
  "home.metric.mockNote": "демо-данные",
  "home.system": "Система",
  "conn.title": "Подключение",
  "conn.refresh": "Обновить список",
  "conn.connect": "Подключить",
  "conn.disconnect": "Отключить",
  "conn.empty": "Устройства не найдены.",
  "conn.emptyHint":
    "Подключите телефон по USB и нажмите «Обновить список». Для проверки интерфейса включите демо-устройство в настройках.",
  "conn.actions": "Проверочные операции",
  "conn.op.identify": "Идентификация",
  "conn.op.list": "Список файлов",
  "conn.op.write": "Тестовая запись",
  "conn.result": "Последняя операция",
  "conn.evidence": "Подтверждение",
  "conn.unavailable": "Недоступно: возможность не поддерживается",
  "conn.retryHint": "можно повторить вручную",
  "dev.serial": "Серийный номер (замаскирован)",
  "dev.transport": "Транспорт",
  "dev.origin": "Источник",
  "table.capability": "Возможность",
  "table.status": "Статус",
  "table.evidence": "Подтверждения",
  "table.operation": "Операция",
  "table.state": "Состояние",
  "table.code": "Код",
  "table.time": "Время",
  "journal.title": "Журнал операций",
  "journal.empty": "Операций пока нет.",
  "settings.title": "Настройки",
  "settings.mockLabel": "Показывать демо-устройство (MOCK)",
  "settings.mockHint": "Тестовое устройство без реального телефона. Всегда помечено MOCK.",
  "settings.logLabel": "Писать журнал в файл",
  "settings.logHint": "Журнал не содержит IMEI и серийных номеров.",
  "settings.about": "О программе",
  "settings.aboutText":
    "YESKIA — независимый клиент с открытым исходным кодом. Не связан с Nokia, HMD Global и Microsoft и не одобрен ими. Nokia и Symbian — товарные знаки их владельцев; упоминаются только для описания совместимости.",
  "settings.version": "Версия",
  "settings.dataDir": "Каталог данных",
  "settings.network": "Сеть",
  "settings.networkNo": "не требуется",
  "settings.networkYes": "требуется",
  "settings.platform": "Платформа",
  "state.loading": "Загрузка…",
  "state.error": "Ошибка",
  "state.saving": "Сохранение…",
  "phone.alt": "Условная иллюстрация телефона",
  "phone.note": "Условная иллюстрация, не фото модели",
} as const;

export type MessageKey = keyof typeof ru;

const en: Record<MessageKey, string> = {
  appName: "YESKIA",
  appTagline: "Client for Nokia / Symbian",
  skipToContent: "Skip to content",
  navLabel: "Main navigation",
  "nav.home": "Home",
  "nav.connection": "Connection",
  "nav.journal": "Journal",
  "nav.settings": "Settings",
  "mode.user": "User",
  "mode.developer": "Developer",
  "mode.label": "Mode",
  "lang.label": "Language",
  "status.devices": "Devices: {n}",
  "status.offline": "Works offline",
  "status.noDevice": "No device connected",
  "status.connected": "Connected: {name}",
  "status.mockOn": "Demo device enabled",
  "status.uiMock": "UI stub",
  "banner.uiMock":
    "UI stub: data does not come from the Rust core (running outside Tauri). Numbers do not describe real devices.",
  "tag.mock": "MOCK",
  "tag.real": "Real",
  "conn.disconnected": "Disconnected",
  "conn.connected": "Connected",
  "conn.connecting": "Connecting…",
  "conn.disconnecting": "Disconnecting…",
  "home.title": "Home",
  "home.noDeviceTitle": "No device selected",
  "home.noDeviceText": "Connect a phone over USB or enable the demo device in Settings.",
  "home.goConnection": "Open “Connection”",
  "home.metrics": "Indicators",
  "home.metric.devices": "Devices found",
  "home.metric.connection": "Connection",
  "home.metric.capabilities": "Capabilities",
  "home.metric.ops": "Operations this session",
  "home.metric.opsValue": "{ok} succeeded · {bad} failed",
  "home.metric.capsValue": "{n} available of {total}",
  "home.metric.mockNote": "demo data",
  "home.system": "System",
  "conn.title": "Connection",
  "conn.refresh": "Refresh list",
  "conn.connect": "Connect",
  "conn.disconnect": "Disconnect",
  "conn.empty": "No devices found.",
  "conn.emptyHint":
    "Connect a phone over USB and press “Refresh list”. To try the interface, enable the demo device in Settings.",
  "conn.actions": "Check operations",
  "conn.op.identify": "Identify",
  "conn.op.list": "List files",
  "conn.op.write": "Test write",
  "conn.result": "Last operation",
  "conn.evidence": "Evidence",
  "conn.unavailable": "Unavailable: capability is not supported",
  "conn.retryHint": "can be retried manually",
  "dev.serial": "Serial number (masked)",
  "dev.transport": "Transport",
  "dev.origin": "Origin",
  "table.capability": "Capability",
  "table.status": "Status",
  "table.evidence": "Evidence",
  "table.operation": "Operation",
  "table.state": "State",
  "table.code": "Code",
  "table.time": "Time",
  "journal.title": "Operation journal",
  "journal.empty": "No operations yet.",
  "settings.title": "Settings",
  "settings.mockLabel": "Show demo device (MOCK)",
  "settings.mockHint": "A test device without a real phone. Always labelled MOCK.",
  "settings.logLabel": "Write log to file",
  "settings.logHint": "The log never contains IMEI or serial numbers.",
  "settings.about": "About",
  "settings.aboutText":
    "YESKIA is an independent open-source client. It is not affiliated with, or endorsed by, Nokia, HMD Global or Microsoft. Nokia and Symbian are trademarks of their owners and are mentioned only to describe compatibility.",
  "settings.version": "Version",
  "settings.dataDir": "Data folder",
  "settings.network": "Network",
  "settings.networkNo": "not required",
  "settings.networkYes": "required",
  "settings.platform": "Platform",
  "state.loading": "Loading…",
  "state.error": "Error",
  "state.saving": "Saving…",
  "phone.alt": "Generic phone illustration",
  "phone.note": "Generic illustration, not a photo of the model",
};

const dictionaries: Record<Lang, Record<MessageKey, string>> = { ru, en };

export type Translate = (key: MessageKey, vars?: Record<string, string | number>) => string;

export function makeTranslate(lang: Lang): Translate {
  const dict = dictionaries[lang];
  return (key, vars) =>
    dict[key].replace(/\{(\w+)\}/g, (_, name: string) => String(vars?.[name] ?? `{${name}}`));
}

export function normalizeLang(v: string | undefined): Lang {
  return v === "en" ? "en" : "ru";
}

const I18nContext = createContext<{ lang: Lang; t: Translate }>({
  lang: "ru",
  t: makeTranslate("ru"),
});

export function I18nProvider({ lang, children }: { lang: Lang; children: ReactNode }) {
  return (
    <I18nContext.Provider value={{ lang, t: makeTranslate(lang) }}>{children}</I18nContext.Provider>
  );
}

export const useI18n = () => useContext(I18nContext);
