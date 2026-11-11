import { useEffect, useState } from "react";
import type { AppModel } from "../useAppModel";
import type { PageId } from "../components/Shell";
import { useI18n } from "../i18n";
import { Panel, ErrorMessage } from "../components/Ui";
import { Dialog } from "../fm/Dialog";
import { toErrorInfo } from "../api";
import type { ErrorInfo } from "../types";
import { defaultProfile, type PrepView, type Profile, type StepStatus } from "../workbench";
const steps = [
  {
    id: "physical",
    ru: "Телефон и безопасность",
    en: "Phone and safety",
    textRu:
      "Телефон уже приобретён. Осмотрите аккумулятор: при вздутии или повреждении не заряжайте его. Проверьте зарядку, разъёмы и клавиши. SIM устанавливайте при выключенном питании. Эти действия выполняются вручную; покупать оборудование мастер не предлагает.",
    textEn:
      "The phone is already purchased. Inspect the battery; do not charge it if swollen or damaged. Check charging, ports and keys. Insert the SIM with power off. These are manual checks; the wizard does not suggest purchases.",
    page: "connection",
  },
  {
    id: "identity",
    ru: "Профиль E72-1",
    en: "E72-1 profile",
    textRu:
      "Исходный профиль стенда: Nokia E72, RM-530, 054.005, S60 3rd FP2 / Symbian 9.3. Сверьте RM и прошивку на телефоне (*#0000#) и сохраните профиль ниже. Имя Bluetooth не подтверждает модель. После обновления до 091.004 или смены телефона старые проверки недействительны.",
    textEn:
      "Initial bench profile: Nokia E72, RM-530, 054.005, S60 3rd FP2 / Symbian 9.3. Check RM and firmware on the phone (*#0000#) and save the profile below. A Bluetooth name does not prove model identity. Firmware 091.004 or a different phone requires fresh checks.",
    page: "connection",
  },
  {
    id: "connection",
    ru: "Подключение и Doctor",
    en: "Connection and Doctor",
    textRu:
      "Для файлов по Bluetooth запустите rshelld на телефоне, затем явно подключитесь к нему в YESKIA. USB PC Suite используется отдельно для чтения PIM. В разделе подключения видны реальный транспорт и доступные операции; Doctor объясняет режим и драйверы. Обнаружение устройства ещё не означает рабочий протокол.",
    textEn:
      "For Bluetooth files, start rshelld on the phone, then explicitly connect in YESKIA. USB PC Suite is a separate route for PIM reads. The connection page shows the actual transport and operations; Doctor explains mode and drivers. Discovery alone does not prove a working protocol.",
    page: "doctor",
  },
  {
    id: "files-backup",
    ru: "Копия доступных файлов",
    en: "Accessible file backup",
    textRu:
      "Создайте копию доступных папок телефона на компьютере до изменений. Дождитесь проверки копии; пропуски или неполная копия требуют разбора. Это файловая копия, она не включает закрытые системные данные и не заменяет PIM-снимок.",
    textEn:
      "Back up accessible phone folders to the computer before changing anything. Wait for verification; skipped files or an incomplete backup need review. This file backup does not include protected system data or replace a PIM snapshot.",
    page: "backups",
  },
  {
    id: "pim-backup",
    ru: "Отдельный снимок контактов PIM",
    en: "Separate PIM contacts snapshot",
    textRu:
      "Через поддерживаемое USB PC Suite соединение прочитайте контакты и экспортируйте снимок на компьютер. Контакты, SMS и календарь — отдельные данные. Если транспорт не умеет их читать, отметьте ограничение, не считайте файловую копию полным backup телефона.",
    textEn:
      "Using a supported USB PC Suite route, read contacts and export the snapshot to the computer. Contacts, SMS and calendar are separate data. If the transport cannot read them, record the limitation; a file copy is not a full phone backup.",
    page: "contacts",
  },
  {
    id: "network",
    ru: "Wi-Fi и мобильная сеть",
    en: "Wi-Fi and mobile network",
    textRu:
      "На E72 настройте сеть 2,4 ГГц и совместимый WPA2 в меню телефона. Для мобильной сети вручную внесите APN вашего оператора. Здесь APN и пароли не запрашиваются и не сохраняются. E72 поддерживает 2G/3G; LTE и 5G отсутствуют. Доступность 2G/3G зависит от оператора.",
    textEn:
      "Configure a 2.4 GHz network and compatible WPA2 in phone settings. Enter your operator's APN manually on the phone. This wizard never requests or stores APN values or passwords. E72 supports 2G/3G, with no LTE or 5G; availability depends on the operator.",
    page: "connection",
  },
  {
    id: "clock",
    ru: "Дата, время, часовой пояс",
    en: "Date, time and timezone",
    textRu:
      "Сверьте дату, время и часовой пояс на телефоне. Ошибка часов влияет на сертификаты и HTTPS. В TLS-мастере есть ручная сверка даты; она не синхронизирует часы автоматически.",
    textEn:
      "Check the phone date, time and timezone. Clock errors affect certificates and HTTPS. The TLS wizard offers a manual date comparison; it does not automatically sync the clock.",
    page: "tls",
  },
  {
    id: "certificates",
    ru: "Сертификаты",
    en: "Certificates",
    textRu:
      "Используйте существующий раздел сертификатов: проверьте издателя, отпечаток и сроки действия перед передачей. Не отключайте проверку сертификатов и не устанавливайте неизвестные корневые сертификаты.",
    textEn:
      "Use the certificates page to inspect issuer, fingerprint and validity before transfer. Keep certificate validation enabled and do not install unknown root certificates.",
    page: "certificates",
  },
  {
    id: "rights",
    ru: "InstallServer / Open4All",
    en: "InstallServer / Open4All",
    textRu:
      "Это ручной шаг для владельца подготовленного телефона. Убедитесь, что InstallServer и Open4All реально работают на вашей прошивке, если планируете unsigned SIS и системный DLL. YESKIA не выполняет jailbreak, не активирует патчи и не выдаёт права из названия модели. При отсутствии прав остановитесь на безопасной передаче файлов.",
    textEn:
      "This is a manual owner step on an already prepared phone. Confirm InstallServer and Open4All actually work on your firmware before using unsigned SIS or a system DLL. YESKIA does not jailbreak, enable patches or infer privileges from a model name. Without verified rights, stay with safe file staging.",
    page: "tls",
  },
  {
    id: "tls",
    ru: "TLS / HTTPS Revival",
    en: "TLS / HTTPS Revival",
    textRu:
      "Откройте TLS-мастер: применимость по платформе, закреплённые SHA-256 пакетов автора, исходный DLL, ручная установка и тест на телефоне. Подготовленный файл не означает установленный компонент. Qt и старые EKA1 имеют отдельные ограничения.",
    textEn:
      "Open the TLS wizard for platform applicability, pinned author packages, original DLL preservation, manual installation and phone testing. A staged file is not an installed component. Qt and older EKA1 have separate limitations.",
    page: "tls",
  },
  {
    id: "contacts",
    ru: "Локальная адресная книга",
    en: "Local address book",
    textRu:
      "Импортируйте VCF или CSV, проверьте колонки и предпросмотр, отредактируйте локальную книгу и разберите дубли вручную. Чтение снимка телефона отделено от локальных правок. Запись контактов на телефон пока не поддерживается.",
    textEn:
      "Import VCF or CSV, review mapping and preview, edit the local book and inspect duplicates manually. Phone snapshots are separate from local edits. Writing contacts to the phone is not yet supported.",
    page: "contacts",
  },
  {
    id: "readiness",
    ru: "Отчёт готовности",
    en: "Readiness report",
    textRu:
      "Экспортируйте отчёт без идентификаторов телефона, APN, паролей и контактов. Ручная отметка не равна проверке устройством. Необязательный или недоступный шаг можно отметить ограничением; мастер не выдаёт такую конфигурацию за полностью проверенную.",
    textEn:
      "Export a report without phone identifiers, APN values, passwords or contacts. A manual report is not device verification. Mark optional or unavailable steps as limitations; the wizard does not call such a setup fully verified.",
    page: "home",
  },
] satisfies { id: string; ru: string; en: string; textRu: string; textEn: string; page: PageId }[];
export function PreparationPage({ model, go }: { model: AppModel; go: (id: PageId) => void }) {
  const { lang } = useI18n();
  const l = (ru: string, en: string) => (lang === "ru" ? ru : en);
  const api = model.backend.workbench;
  const [view, setView] = useState<PrepView | null>(null);
  const [profile, setProfile] = useState<Profile>(defaultProfile);
  const [selected, setSelected] = useState("physical");
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [pending, setPending] = useState<Exclude<StepStatus, "verified"> | null>(null);
  const devices = model.devices.filter((d) => d.connection === "connected");
  const [chosen, setChosen] = useState("");
  const device = devices.find((d) => d.id === chosen) ?? devices[0];
  const deviceId = device?.id ?? null;
  const fingerprint = device?.matrix?.device_fingerprint ?? "offline";
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    let active = true;
    setView(null);
    void api
      .preparation({ kind: "load", device_id: deviceId })
      .then((v) => {
        if (active) {
          setView(v);
          setProfile(v.context.profile);
        }
      })
      .catch((e) => {
        if (active) setError(toErrorInfo(e));
      });
    return () => {
      active = false;
      setPending(null);
    };
  }, [api, deviceId, fingerprint, model.settings?.developer_mode]);
  const step = steps.find((s) => s.id === selected)!;
  const state = view?.progress.value.steps[step.id];
  const label = (status: StepStatus) =>
    ({
      "not-started": l("Не начато", "Not started"),
      manual: l("Ручная отметка", "Manual report"),
      verified: l("Подтверждено устройством", "Device verified"),
      blocked: l("Заблокировано", "Blocked"),
      "not-supported": l("Не поддерживается", "Not supported"),
    })[status];
  return (
    <div className="workspace-page">
      <div className="page-heading">
        <h1>{l("Подготовка E72", "Prepare E72")}</h1>
        <span className="tag">E72-1</span>
      </div>
      <p className="hint">
        {l(
          "Подготовка уже купленного телефона. Шаги сохраняются отдельно для телефона, прошивки, транспорта, драйверов, режима YESKIA и версии TLS-пакетов.",
          "Prepare the phone you already own. Progress is scoped to phone, firmware, transport, drivers, YESKIA mode and TLS manifest version.",
        )}
      </p>
      {error && <ErrorMessage error={error} />} {message && <p role="status">{message}</p>}
      <div className="btn-row">
        <label>
          {l("Соединение для проверки", "Connection under test")}
          <select
            disabled={busy}
            value={deviceId ?? ""}
            onChange={(e) => setChosen(e.target.value)}
          >
            {!devices.length && (
              <option value="">
                {l("Офлайн: ручная подготовка", "Offline: manual preparation")}
              </option>
            )}
            {devices.map((d) => (
              <option key={d.id} value={d.id}>
                {d.display_name} · {d.transport}
              </option>
            ))}
          </select>
        </label>
        <span className="tag">{view?.context.transport ?? "offline"}</span>
        <button
          className="btn btn-secondary"
          disabled={busy}
          onClick={() =>
            void run(async () =>
              setView(await api.preparation({ kind: "load", device_id: deviceId })),
            )
          }
        >
          {l("Обновить состояние", "Refresh status")}
        </button>
        <button
          className="btn btn-secondary"
          disabled={busy || !view}
          onClick={() =>
            void run(async () => {
              const report = await api.exportPreparation(deviceId);
              const path = await model.backend.pickSavePath("YESKIA-E72-readiness.json");
              if (path) {
                await model.backend.savePersonalSnapshot(path, JSON.stringify(report, null, 2));
                setMessage(l("Отчёт сохранён: ", "Report saved: ") + path);
              }
            })
          }
        >
          {l("Экспорт отчёта готовности", "Export readiness report")}
        </button>
      </div>
      <details className="panel">
        <summary>{l("Профиль стенда (ввод владельца)", "Bench profile (owner input)")}</summary>
        <div className="form-grid">
          <label>
            {l("Модель", "Model")}
            <select
              value={profile.model}
              onChange={(e) => setProfile({ ...profile, model: e.target.value })}
            >
              {[
                "nokia-e72",
                "nokia-e52",
                "nokia-n95",
                "nokia-n95-8gb",
                "nokia-n97-mini",
                "nokia-n8",
                "nokia-e7",
                "nokia-7610",
              ].map((m) => (
                <option key={m}>{m}</option>
              ))}
            </select>
          </label>
          <label>
            RM
            <input
              value={profile.rm}
              maxLength={16}
              onChange={(e) => setProfile({ ...profile, rm: e.target.value })}
            />
          </label>
          <label>
            {l("Прошивка", "Firmware")}
            <input
              value={profile.firmware}
              maxLength={32}
              onChange={(e) => setProfile({ ...profile, firmware: e.target.value })}
            />
          </label>
          <label>
            Symbian
            <select
              value={profile.os}
              onChange={(e) => setProfile({ ...profile, os: e.target.value })}
            >
              {["9.2", "9.3", "9.4", "symbian3", "eka1"].map((v) => (
                <option key={v}>{v}</option>
              ))}
            </select>
          </label>
        </div>
        <p className="hint">
          {l(
            "Профиль не извлекается из имени Bluetooth. Сохранение изменённой прошивки или платформы открывает новый контекст проверок.",
            "The profile is not inferred from the Bluetooth name. Changing firmware or platform opens a new verification context.",
          )}
        </p>
        <button
          className="btn"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              await api.call({ kind: "prep_save_profile", profile });
              setView(await api.preparation({ kind: "load", device_id: deviceId }));
              setMessage(
                l(
                  "Профиль сохранён; проверьте статусы текущего контекста.",
                  "Profile saved; review statuses for the current context.",
                ),
              );
            })
          }
        >
          {l("Сохранить профиль", "Save profile")}
        </button>
      </details>
      <div className="preparation-grid">
        <Panel title={l("Шаги подготовки", "Preparation steps")}>
          <div className="preparation-steps">
            {steps.map((s, i) => (
              <button
                className={`record-row ${s.id === selected ? "selected" : ""}`}
                key={s.id}
                aria-current={s.id === selected ? "step" : undefined}
                onClick={() => setSelected(s.id)}
              >
                <strong>
                  {i + 1}. {l(s.ru, s.en)}
                </strong>
                <small>{label(view?.progress.value.steps[s.id]?.status ?? "not-started")}</small>
              </button>
            ))}
          </div>
        </Panel>
        <Panel title={l(step.ru, step.en)}>
          <p>{l(step.textRu, step.textEn)}</p>
          <p className="tag">{label(state?.status ?? "not-started")}</p>
          {state && (
            <p className="hint">
              {state.source} · {new Date(state.checked_at_ms).toLocaleString(lang)}
            </p>
          )}
          <div className="btn-row">
            <button className="btn" onClick={() => go(step.page)}>
              {l("Открыть связанный раздел", "Open related page")}
            </button>
          </div>
          <p>
            {l(
              "После ручной проверки выберите отметку. Подтверждение устройством появляется только от реальной завершённой операции.",
              "After checking manually, choose a report. Device verification comes only from an actual completed native operation.",
            )}
          </p>
          <div className="btn-row">
            {(["manual", "blocked", "not-supported", "not-started"] as const).map((status) => (
              <button
                className="btn btn-secondary"
                key={status}
                disabled={busy || !view}
                onClick={() => setPending(status)}
              >
                {label(status)}
              </button>
            ))}
          </div>
          {state?.status === "verified" && (
            <p className="hint">
              {l(
                "Это подтверждение относится только к указанному источнику и текущему контексту, не ко всем функциям телефона.",
                "This verification applies only to its stated source and current context, not every phone function.",
              )}
            </p>
          )}
        </Panel>
      </div>
      {pending && view && (
        <Dialog
          title={l("Подтвердить ручную отметку", "Confirm manual report")}
          onClose={() => setPending(null)}
          actions={[
            { label: l("Отмена", "Cancel"), onClick: () => setPending(null) },
            {
              label: l("Сохранить отметку", "Save report"),
              primary: true,
              onClick: () => {
                const status = pending;
                setPending(null);
                void run(async () =>
                  setView(
                    await api.preparation({
                      kind: "record",
                      device_id: deviceId,
                      revision: view.progress.revision,
                      expected_key: view.key,
                      step: selected,
                      status,
                    }),
                  ),
                );
              },
            },
          ]}
        >
          <p>
            {l(step.ru, step.en)}: {label(pending)}
          </p>
          <p>
            {l(
              "Это отчёт владельца, а не аппаратное доказательство.",
              "This is an owner report, not hardware evidence.",
            )}
          </p>
        </Dialog>
      )}
    </div>
  );
}
