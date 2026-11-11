import { useI18n } from "./i18n";
import type { ErrorInfo } from "./types";
const ru = {
  "error.TLS_PIN_MISMATCH_REVIEW_UPSTREAM_VERSION":
    "Хеш или размер отличается от проверенной версии. Импорт остановлен; нужна проверка новой версии автора.",
  "error.TLS_UNSAFE_ZIP":
    "Архив содержит небезопасный путь или ссылку. Используйте проверенный файл автора.",
  "error.TLS_UNEXPECTED_ZIP": "В архиве неожиданные файлы. Импорт остановлен.",
  "error.TLS_SIS_": "Пакет SIS не прошёл проверку UID, версии, платформы или контрольных сумм.",
  "error.TLS_BROKEN_": "Пакет повреждён или имеет неподдерживаемую структуру.",
  "error.LOCAL_EDIT_CONFLICT_RELOAD":
    "Данные изменились в другом окне. Перезагрузите книгу перед сохранением.",
  "error.CONTACT_IMPORT_INVALID":
    "Файл контактов не разобран. Проверьте формат, кодировку и разделитель; книга не изменена.",
  "error.CONTACT_DOCUMENT_INVALID":
    "Карточка содержит недопустимые поля или превышает лимит. Проверьте введённые данные.",
  "error.CONTACT_BOOK_LIMIT": "Превышен допустимый объём книги контактов.",
  "error.PREPARATION_CONTEXT_CHANGED_RELOAD":
    "Телефон или профиль изменился. Обновите состояние перед отметкой.",
  "error.CHOOSE_NEW_EXPORT_FILENAME":
    "Этот путь недоступен или файл уже существует. Выберите новое имя экспорта.",
  "error.TLS_MODEL_MISMATCH": "Модель, прочитанная с телефона, не совпадает с выбранным профилем.",
  "error.TLS_ORIGINAL_DLL_SNAPSHOT_REQUIRED":
    "Сначала сохраните исходный DLL и подтвердите ручной снимок.",
  "error.TLS_MBEDTLS_INSTALL_REPORT_REQUIRED":
    "Сначала установите MBedTLS на телефоне и подтвердите этот шаг.",

  "device.title": "Телефон и подключение",
  "device.connection": "Подключение",
  "device.diagnostics": "Диагностика",
  "personal.read": "Прочитать телефон",
  "personal.reading": "Чтение…",
  "personal.cancel": "Отменить чтение",
  "personal.search": "Поиск записей",
  search: "Поиск",
  "personal.export": "Экспорт JSON",
  "personal.calendarSection": "Раздел календаря",
  "personal.events": "События",
  "personal.tasks": "Задачи",
  "personal.details": "Подробности",
  "personal.select": "Выберите запись из списка.",
  "personal.noName": "Без названия",
  "personal.empty": "Записей не найдено",
  "personal.start": "Нажмите «Прочитать телефон», чтобы загрузить записи.",
  "personal.connect": "Подключите телефон в разделе «Подключение».",
  "personal.ready": "Телефон подключён. Данные читаются напрямую по USB.",
  saved: "Сохранено",
  yes: "Да",
  no: "Нет",
  "field.name": "Имя",
  "field.given": "Имя",
  "field.family": "Фамилия",
  "field.phones": "Телефоны",
  "field.emails": "Почта",
  "field.number": "Номер",
  "field.text": "Сообщение",
  "field.date": "Дата",
  "field.unread": "Непрочитанное",
  "field.read": "Прочитанное",
  "field.unsent": "Не отправлено",
  "field.sent": "Отправлено",
  "field.state": "Состояние",
  "field.direction": "Направление",
  "field.summary": "Название",
  "field.description": "Описание",
  "field.location": "Место",
  "field.start": "Начало",
  "field.end": "Окончание",
  "field.due": "Срок",
  "field.status": "Статус",
  "field.priority": "Приоритет",
  "field.recurrence": "Повторение",
  "field.part": "Часть",
  "field.parts": "Всего частей",
  "field.incoming": "Входящее",
  "field.outgoing": "Исходящее",
  "cert.title": "Сертификаты S60",
  "cert.inspect": "Проверка сертификата",
  "cert.intro":
    "Откройте публичный сертификат CER, DER или PEM, чтобы проверить издателя, срок действия и отпечаток.",
  "cert.open": "Открыть сертификат",
  "cert.install": "Установить на телефон",
  "cert.trust": "Доверие этого телефона к сертификату проверяется отдельно.",
  "cert.help": "Ошибка сертификата при установке",
  "cert.advice":
    "Проверьте дату и время телефона и получите актуальный пакет у издателя. Сертификаты внутри SIS/SISX можно проверить в разделе «Приложения → Мои установщики».",
  "cert.issuer": "Издатель",
  "cert.subject": "Владелец",
  "cert.serial": "Серийный номер",
  "cert.not_before": "Действует с",
  "cert.not_after": "Действует до",
  "cert.validity": "Срок действия",
  "cert.key_type": "Ключ",
  "cert.self_issued": "Самоизданный",
  "cert.valid": "Действует",
  "cert.expired": "Истёк",
  "cert.not-yet-valid": "Ещё не действует",
  "cert.confirm": "Передать сертификат для установки?",
  "cert.confirmText":
    "Телефон запросит приём файла. Откройте сертификат на телефоне и подтвердите установку и нужные области доверия. Передавайте только сертификаты, источнику которых доверяете.",
  "cert.send": "Передать на телефон",
  cancel: "Отмена",
  "cert.sent":
    "Сертификат передан. Откройте его на телефоне и завершите установку. Установка ещё не подтверждена.",
  "cert.sending": "Обработка…",
  "cert.connect": "Подключите телефон, чтобы передать сертификат для установки.",
  "error.CERT_INVALID": "Выберите один публичный сертификат CER, DER или PEM до 512 КБ.",
  "error.OBEX_INTERFACE_MISSING":
    "Интерфейс Nokia USB OBEX не найден. Разблокируйте телефон, выберите режим PC Suite и проверьте драйвер Nokia.",
  "error.OBEX_BUSY": "USB-канал занят. Закройте Nokia PC Suite и повторите.",
  "error.PCCS_TARGET":
    "Телефон не открыл канал контактов и календаря. Выберите режим PC Suite и повторите.",
  "error.PCCS_INVALID_RESPONSE":
    "Ответ телефона неполный или имеет неподдерживаемый формат. Записи не показаны.",
  "error.PCCS_REFUSED": "Телефон отклонил чтение данных.",
  "error.PCCS_STORE_REFUSED": "Телефон не открыл выбранное хранилище.",
  "error.PCCS_FORMAT_REFUSED": "Телефон не принял формат чтения.",
  "error.SMS_PORT_MISSING":
    "На этом подключении нет порта для чтения SMS. Проверьте режим PC Suite и драйвер Nokia.",
  "error.SMS_READ_UNAVAILABLE": "Телефон не поддержал чтение SMS с сохранением их статуса.",
  "error.SMS_PDU_MODE_REQUIRED":
    "Телефон использует текстовый режим SMS. Чтение этого режима пока не поддерживается.",
  "error.SMS_INVALID_RESPONSE":
    "Не удалось разобрать сообщения телефона. Частичный список не показан.",
  "error.SMS_BINARY": "В хранилище есть сообщения в неподдерживаемой бинарной кодировке.",
  "error.NATIVE_WINDOWS_REQUIRED": "Подключение телефона доступно в приложении YESKIA для Windows.",
  "error.CERT_OPP_UNAVAILABLE":
    "Телефон не открыл приём сертификата по USB. Убедитесь, что он разблокирован и разрешает приём файлов.",
  "error.CERT_TRANSFER_UNCERTAIN":
    "Подтверждение передачи потеряно. Проверьте входящие на телефоне перед повтором.",
  "error.CERT_PHONE_REFUSED": "Телефон отклонил сертификат.",
  "error.PIM_BUSY": "Дождитесь завершения текущей операции с телефоном.",
  "store.title": "Приложения",
  "store.catalog": "Каталог",
  "store.local": "Мои установщики",
  "store.search": "Поиск приложений",
  "store.find": "Найти приложение",
  "store.category": "Категория",
  "store.all": "Все категории",
  "store.count": "приложений",
  "store.preview":
    "Каталог s60modern · Предварительная версия. Загрузка и установка появятся после подключения сервиса каталога.",
  "store.details": "Подробнее",
  "store.unavailable": "Загрузка пока недоступна.",
};
const en: Record<keyof typeof ru, string> = {
  "error.TLS_PIN_MISMATCH_REVIEW_UPSTREAM_VERSION":
    "Hash or size differs from the reviewed version. Import stopped; an upstream update needs review.",
  "error.TLS_UNSAFE_ZIP":
    "The archive contains an unsafe path or link. Use the reviewed author file.",
  "error.TLS_UNEXPECTED_ZIP": "The archive contains unexpected files. Import stopped.",
  "error.TLS_SIS_": "SIS UID, version, platform or checksum validation failed.",
  "error.TLS_BROKEN_": "The package is damaged or has an unsupported structure.",
  "error.LOCAL_EDIT_CONFLICT_RELOAD":
    "Another window changed this data. Reload the book before saving.",
  "error.CONTACT_IMPORT_INVALID":
    "Contacts could not be parsed. Check format, encoding and delimiter; the book was not changed.",
  "error.CONTACT_DOCUMENT_INVALID":
    "The contact has invalid fields or exceeds a limit. Review the entered values.",
  "error.CONTACT_BOOK_LIMIT": "The contact book exceeds its size limit.",
  "error.PREPARATION_CONTEXT_CHANGED_RELOAD":
    "The phone or profile changed. Refresh before recording a report.",
  "error.CHOOSE_NEW_EXPORT_FILENAME":
    "The path is inaccessible or the file exists. Choose a new export filename.",
  "error.TLS_MODEL_MISMATCH": "The model read from the phone does not match the selected profile.",
  "error.TLS_ORIGINAL_DLL_SNAPSHOT_REQUIRED":
    "Preserve the original DLL and confirm the manual snapshot first.",
  "error.TLS_MBEDTLS_INSTALL_REPORT_REQUIRED":
    "Install MBedTLS on the phone and report that step first.",

  "device.title": "Phone and connection",
  "device.connection": "Connection",
  "device.diagnostics": "Diagnostics",
  "personal.read": "Read phone",
  "personal.reading": "Reading…",
  "personal.cancel": "Cancel reading",
  "personal.search": "Search records",
  search: "Search",
  "personal.export": "Export JSON",
  "personal.calendarSection": "Calendar section",
  "personal.events": "Events",
  "personal.tasks": "Tasks",
  "personal.details": "Details",
  "personal.select": "Select a record from the list.",
  "personal.noName": "Untitled",
  "personal.empty": "No records found",
  "personal.start": "Choose “Read phone” to load records.",
  "personal.connect": "Connect your phone in the Connection section.",
  "personal.ready": "Phone connected. Records are read directly over USB.",
  saved: "Saved",
  yes: "Yes",
  no: "No",
  "field.name": "Name",
  "field.given": "First name",
  "field.family": "Last name",
  "field.phones": "Phone numbers",
  "field.emails": "Email",
  "field.number": "Number",
  "field.text": "Message",
  "field.date": "Date",
  "field.unread": "Unread",
  "field.read": "Read",
  "field.unsent": "Unsent",
  "field.sent": "Sent",
  "field.state": "State",
  "field.direction": "Direction",
  "field.summary": "Title",
  "field.description": "Description",
  "field.location": "Location",
  "field.start": "Start",
  "field.end": "End",
  "field.due": "Due",
  "field.status": "Status",
  "field.priority": "Priority",
  "field.recurrence": "Recurrence",
  "field.part": "Part",
  "field.parts": "Total parts",
  "field.incoming": "Incoming",
  "field.outgoing": "Outgoing",
  "cert.title": "S60 certificates",
  "cert.inspect": "Inspect certificate",
  "cert.intro":
    "Open a public CER, DER or PEM certificate to check its issuer, validity and fingerprint.",
  "cert.open": "Open certificate",
  "cert.install": "Install on phone",
  "cert.trust": "Trust on this phone must be checked separately.",
  "cert.help": "Certificate error during installation",
  "cert.advice":
    "Check the phone's date and time and obtain a current package from the publisher. Inspect certificates embedded in SIS/SISX under Applications → My installers.",
  "cert.issuer": "Issuer",
  "cert.subject": "Subject",
  "cert.serial": "Serial number",
  "cert.not_before": "Valid from",
  "cert.not_after": "Valid until",
  "cert.validity": "Validity",
  "cert.key_type": "Key",
  "cert.self_issued": "Self-issued",
  "cert.valid": "Valid",
  "cert.expired": "Expired",
  "cert.not-yet-valid": "Not yet valid",
  "cert.confirm": "Send certificate for installation?",
  "cert.confirmText":
    "Accept the incoming file on your phone, open the certificate and confirm installation and the required trust settings. Only send certificates from a source you trust.",
  "cert.send": "Send to phone",
  cancel: "Cancel",
  "cert.sent":
    "Certificate delivered. Open it on your phone to finish installation. Installation has not been confirmed yet.",
  "cert.sending": "Working…",
  "cert.connect": "Connect a phone to send the certificate for installation.",
  "error.CERT_INVALID": "Choose one public CER, DER or PEM certificate up to 512 KiB.",
  "error.OBEX_INTERFACE_MISSING":
    "No Nokia USB OBEX interface found. Unlock your phone, select PC Suite mode and check the Nokia driver.",
  "error.OBEX_BUSY": "The USB channel is busy. Close Nokia PC Suite and try again.",
  "error.PCCS_TARGET":
    "The phone did not open the contacts and calendar channel. Select PC Suite mode and try again.",
  "error.PCCS_INVALID_RESPONSE":
    "The phone returned incomplete or unsupported data. No records were published.",
  "error.PCCS_REFUSED": "The phone refused the read request.",
  "error.PCCS_STORE_REFUSED": "The phone did not open the selected store.",
  "error.PCCS_FORMAT_REFUSED": "The phone did not accept the requested read format.",
  "error.SMS_PORT_MISSING":
    "This connection has no SMS port. Check PC Suite mode and the Nokia driver.",
  "error.SMS_READ_UNAVAILABLE":
    "The phone did not support reading SMS while preserving their read status.",
  "error.SMS_PDU_MODE_REQUIRED": "The phone uses SMS text mode, which is not supported yet.",
  "error.SMS_INVALID_RESPONSE":
    "The phone's messages could not be decoded. No partial list was published.",
  "error.SMS_BINARY": "The store contains messages with an unsupported binary encoding.",
  "error.NATIVE_WINDOWS_REQUIRED": "Phone connections are available in YESKIA for Windows.",
  "error.CERT_OPP_UNAVAILABLE":
    "The phone did not open USB certificate delivery. Unlock it and allow incoming files.",
  "error.CERT_TRANSFER_UNCERTAIN":
    "The delivery acknowledgement was lost. Check the phone's inbox before retrying.",
  "error.CERT_PHONE_REFUSED": "The phone refused the certificate.",
  "error.PIM_BUSY": "Wait for the current phone operation to finish.",
  "store.title": "Applications",
  "store.catalog": "Catalog",
  "store.local": "My installers",
  "store.search": "Search applications",
  "store.find": "Find an application",
  "store.category": "Category",
  "store.all": "All categories",
  "store.count": "applications",
  "store.preview":
    "s60modern catalog · Preview. Downloads and installation will be available when the catalog service is connected.",
  "store.details": "Details",
  "store.unavailable": "Downloads are not available yet.",
};
export type WorkspaceKey = keyof typeof ru;
export function useWorkspaceCopy() {
  const { lang } = useI18n();
  const dict = lang === "ru" ? ru : en;
  return {
    lang,
    c: (key: WorkspaceKey) => dict[key],
    error: (error: ErrorInfo): ErrorInfo => {
      const key = (Object.keys(dict) as WorkspaceKey[]).find(
        (k) => k.startsWith("error.") && error.message.includes(k.slice(6)),
      );
      if (key) return { ...error, message: dict[key] };
      if (lang === "en") {
        const messages: Record<string, string> = {
          NOT_CONNECTED: "Connect a phone first.",
          DEVICE_DISCONNECTED: "The phone was disconnected. Reconnect and try again.",
          DEVICE_NOT_FOUND: "The selected phone is no longer available.",
          TIMEOUT: "The phone did not respond in time.",
          OPERATION_CANCELLED: "Operation cancelled.",
          HOST_ACCESS: "The device or selected file is busy or inaccessible.",
          FINGERPRINT_MISMATCH: "The device or file changed. Inspect it again before proceeding.",
          TRANSPORT_ERROR:
            "The phone response could not be read. Check the connection and try again.",
          OPERATION_FAILED: "The phone operation failed. Check the connection before retrying.",
          STORAGE_ERROR: "The file could not be saved. Check the selected folder.",
          ALREADY_EXISTS: "This file already exists. Choose a new name.",
          INVALID_ARGUMENT: "The selected file or request is not supported.",
          CAPABILITY_UNSUPPORTED: "This operation is not supported by the current connection.",
        };
        if (messages[error.code]) return { ...error, message: messages[error.code] };
      }
      return error;
    },
  };
}
