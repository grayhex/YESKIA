import { useCallback, useEffect, useRef, useState } from "react";
import { useI18n } from "../i18n";
import type { AppModel } from "../useAppModel";
import type { PageId } from "../components/Shell";
import { Panel, ErrorMessage } from "../components/Ui";
import { Dialog } from "../fm/Dialog";
import { toErrorInfo } from "../api";
import type { ErrorInfo } from "../types";
import { findPhone } from "../phones/registry";
import { selectDeviceForOperation } from "../deviceRouting";
import {
  defaultProfile,
  type Profile,
  type Target,
  type Applicability,
  type TlsComponent,
  type TlsProgress,
  type Versioned,
} from "../workbench";
export function TlsPage({ model, go }: { model: AppModel; go: (id: PageId) => void }) {
  const { lang } = useI18n();
  const l = (ru: string, en: string) => (lang === "ru" ? ru : en);
  const labels: Record<string, [string, string]> = {
    manual: ["Ручная проверка", "Manual check"],
    documented: ["По документации автора", "Author documented"],
    blocked: ["Заблокировано", "Blocked"],
    unsupported: ["Не поддерживается", "Unsupported"],
    "qt-is-not-covered-by-system-ssl": [
      "Системный SSL-патч не исправляет Qt.",
      "The system SSL patch does not fix Qt.",
    ],
    "unknown-application-stack": [
      "Сетевой стек приложения не определён.",
      "The application network stack is unknown.",
    ],
    "symbian3-author-support; model-and-firmware-not-verified": [
      "Symbian^3 указан автором; эта модель и прошивка не проверены.",
      "The author lists Symbian^3; this model and firmware are unverified.",
    ],
    "eka1-needs-separate-bearssl-packages": [
      "Для EKA1 нужны отдельные BearSSL-пакеты.",
      "EKA1 needs separate BearSSL packages.",
    ],
    "symbian-version-mismatch": [
      "Версия Symbian не совпадает с профилем модели.",
      "The Symbian version conflicts with the model profile.",
    ],
    "rm-mismatch": [
      "RM не совпадает с выбранной моделью.",
      "RM conflicts with the selected model.",
    ],
    "identity-not-confirmed-by-device": [
      "Модель не подтверждена чтением с телефона.",
      "The model is not confirmed by a phone read.",
    ],
    "exact-firmware-not-verified": [
      "Точная прошивка ещё не проверена.",
      "The exact firmware is not yet verified.",
    ],
    "unknown-platform": [
      "Поддержка этой платформы не определена.",
      "Support for this platform is unknown.",
    ],
    "installserver-required-for-unsigned-sis": [
      "Для SIS без подписи нужен рабочий InstallServer.",
      "The unsigned SIS needs a working InstallServer.",
    ],
    "open4all-or-equivalent-rights-required; snapshot-before-system-change": [
      "Для системного DLL нужны подтверждённые права и исходная копия до изменения.",
      "The system DLL needs confirmed privileges and an original snapshot before changes.",
    ],
    "installed-reported": ["Установка SIS отмечена владельцем", "Owner reported SIS installation"],
    "applied-reported": ["Применение DLL отмечено владельцем", "Owner reported DLL application"],
    "restored-reported": ["Восстановление отмечено владельцем", "Owner reported restoration"],
    "snapshot-reported": ["Исходная копия отмечена владельцем", "Owner reported original snapshot"],
    "passed-reported": ["Успех отмечен владельцем", "Owner reported success"],
    failed: ["Ошибка", "Failed"],
    "not-tested": ["Не проверялось", "Not tested"],
    "not-confirmed": ["Не подтверждено", "Not confirmed"],
    "transferred-read-back-verified": [
      "Файл передан и проверен обратным чтением",
      "File staged and read-back verified",
    ],
    "transferred-on-mock": ["Передано только на MOCK", "Staged on MOCK only"],
    "unknown-check-phone-before-retry": [
      "Исход неизвестен — проверьте телефон перед повтором",
      "Unknown outcome — inspect the phone before retrying",
    ],
  };
  const status = (code: string) =>
    labels[code] ? l(...labels[code]) : l("Нет подтверждённых данных", "No verified data");
  const api = model.backend.workbench;
  const [profile, setProfile] = useState<Profile>(defaultProfile);
  const [components, setComponents] = useState<TlsComponent[]>([]);
  const [stack, setStack] = useState("system-native");
  const [applies, setApplies] = useState<Applicability | null>(null);
  const [progress, setProgress] = useState<Versioned<TlsProgress> | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [message, setMessage] = useState("");
  const [dll, setDll] = useState("");
  const [clock, setClock] = useState("");
  const [clockChecked, setClockChecked] = useState(false);
  const [confirm, setConfirm] = useState<{
    title: string;
    text: string;
    run: () => Promise<void>;
  } | null>(null);
  const [operation, setOperation] = useState<string | null>(null);
  const generation = useRef(0);
  const invalidate = useCallback(() => {
    generation.current++;
  }, []);
  const device = selectDeviceForOperation(model.devices, "files.write");
  const fingerprint = device?.matrix?.device_fingerprint ?? "offline";
  const scope = [profile.model, profile.rm, profile.os, profile.firmware].join("|");
  const target: Target = {
    ...profile,
    identity_confirmed:
      device?.origin === "real" && findPhone(device.identified_model)?.id === profile.model,
  };
  const load = async () => {
    const stamp = generation.current;
    const [nextComponents, nextProgress] = await Promise.all([
      api.call({ kind: "tls_state" }),
      api.call({ kind: "tls_progress", fingerprint, firmware: scope }),
    ]);
    if (stamp === generation.current) {
      setComponents(nextComponents);
      setProgress(nextProgress);
    }
  };
  useEffect(() => {
    let active = true;
    void api
      .call({ kind: "prep_profile" })
      .then((p) => {
        if (active) setProfile(p.value);
      })
      .catch((e) => {
        if (active) setError(toErrorInfo(e));
      });
    return () => {
      active = false;
      invalidate();
    };
  }, [api, invalidate]);
  useEffect(() => {
    let active = true;
    setProgress(null);
    setConfirm(null);
    setClockChecked(false);
    setClock("");
    setDll("");
    void Promise.all([
      api.call({ kind: "tls_state" }),
      api.call({ kind: "tls_progress", fingerprint, firmware: scope }),
      api.call({
        kind: "tls_applicability",
        target: {
          ...profile,
          identity_confirmed:
            device?.origin === "real" && findPhone(device.identified_model)?.id === profile.model,
        },
        stack,
      }),
    ])
      .then(([c, p, a]) => {
        if (active) {
          setComponents(c);
          setProgress(p);
          setApplies(a);
        }
      })
      .catch((e) => {
        if (active) setError(toErrorInfo(e));
      });
    return () => {
      active = false;
      invalidate();
    };
  }, [
    api,
    fingerprint,
    profile,
    stack,
    device?.identified_model,
    device?.origin,
    scope,
    invalidate,
  ]);
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setMessage("");
    try {
      await fn();
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      setBusy(false);
    }
  };
  const record = (item: string, status: string) =>
    setConfirm({
      title: l("Ручная отметка", "Manual report"),
      text: l(
        "Подтверждаю, что выполнил этот шаг на телефоне. Эта отметка не подтверждает результат устройством.",
        "I confirm I performed this step on the phone. This report does not create device verification.",
      ),
      run: async () => {
        if (!progress) return;
        setProgress(
          await api.call({
            kind: "tls_record",
            fingerprint,
            firmware: scope,
            revision: progress.revision,
            item,
            status,
            confirm: true,
          }),
        );
      },
    });
  const stage = (component: TlsComponent) =>
    setConfirm({
      title: l("Передать установщик на телефон?", "Stage installer on phone?"),
      text: l(
        "Будет создан C:/Data/MBedTLS.sis с обратным чтением. Существующий файл не заменяется. Установку затем нужно подтвердить на телефоне.",
        "Create C:/Data/MBedTLS.sis with read-back verification. Existing files are not replaced. Confirm installation on the phone afterwards.",
      ),
      run: async () => {
        if (!device) return;
        const stamp = generation.current;
        const id = await api.stageTls(device.id, component.id, target);
        setOperation(id);
        try {
          const result = await model.backend.awaitOperation(id);
          if (result.outcome !== "success" || result.uncertain)
            throw (
              result.error ??
              Error(
                l(
                  "Исход записи неизвестен. Проверьте файл на телефоне перед повтором.",
                  "Write outcome is unknown. Inspect the phone before retrying.",
                ),
              )
            );
          if (stamp === generation.current) {
            setMessage(
              l(
                "Файл передан и проверен. Пакет ещё не установлен.",
                "File staged and verified. The package is not installed yet.",
              ),
            );
            await load();
          }
        } finally {
          setOperation(null);
        }
      },
    });
  useEffect(() => {
    if (progress?.value.phone_clock) setClock(progress.value.phone_clock);
  }, [progress?.value.phone_clock]);
  const unsupported = !applies || ["unsupported", "blocked"].includes(applies.status);
  const hasSnapshot = progress?.value.snapshot?.status === "snapshot-reported";
  const sis = progress?.value.components["mbedtls-lib"]?.status === "installed-reported";
  return (
    <div className="workspace-page">
      <div className="page-heading">
        <h1>TLS / HTTPS Revival</h1>
        <div className="btn-row">
          <button
            className="btn btn-secondary"
            disabled={busy || !progress}
            onClick={() =>
              void run(async () => {
                const report = await api.call({ kind: "tls_report", fingerprint, profile, stack });
                const path = await model.backend.pickSavePath("YESKIA-TLS-report.json");
                if (path) {
                  await model.backend.savePersonalSnapshot(path, JSON.stringify(report, null, 2));
                  setMessage(l("Отчёт сохранён: ", "Report saved: ") + path);
                }
              })
            }
          >
            {l("Экспорт отчёта TLS", "Export TLS report")}
          </button>
          <button className="btn btn-secondary" onClick={() => go("preparation")}>
            {l("Подготовка E72", "Prepare E72")}
          </button>
        </div>
      </div>
      <p className="hint">
        {l(
          "Локальный мастер на основе s60modern. Скачайте файлы по ссылке автора и импортируйте их для проверки. Двоичные файлы не входят в YESKIA.",
          "Local wizard based on s60modern. Download files from the author and import them for validation. Binaries are not bundled with YESKIA.",
        )}
      </p>
      {error && <ErrorMessage error={error} />} {message && <p role="status">{message}</p>}
      {busy && <p role="status">{l("Выполняется…", "Working…")}</p>}
      {operation && (
        <button
          className="btn btn-secondary"
          onClick={() => void model.backend.cancelOperation(operation)}
        >
          {l("Отменить операцию", "Cancel operation")}
        </button>
      )}
      <div className="workbench-grid">
        <Panel title={l("1. Телефон и область действия", "1. Phone and scope")}>
          <p>
            {profile.model} · {profile.rm} · {profile.firmware} · Symbian {profile.os}
          </p>
          <p>
            {l(
              "Профиль владельца; прошивка не считается проверенной автоматически.",
              "Owner profile; firmware is not automatically verified.",
            )}
          </p>
          <button className="btn btn-secondary" onClick={() => go("preparation")}>
            {l("Изменить профиль", "Edit profile")}
          </button>
          <label>
            {l("Сетевой стек приложения", "Application network stack")}
            <select value={stack} onChange={(e) => setStack(e.target.value)}>
              <option value="system-native">
                {l("Системный SSL / native S60", "System SSL / native S60")}
              </option>
              <option value="j2me">Java ME</option>
              <option value="mbedtls-native">
                {l("Приложение со своим MBedTLS", "App linked to MBedTLS")}
              </option>
              <option value="qt">Qt</option>
            </select>
          </label>
          {applies && (
            <>
              <p className="tag">{status(applies.status)}</p>
              <p>
                {l(
                  "Точная прошивка и этот телефон ещё не проверены. Совместимость из документации автора не равна аппаратной проверке.",
                  "Exact firmware and this phone are not verified. Author documentation is not hardware evidence.",
                )}
              </p>
              {applies.reasons.map((r) => (
                <p className="hint" key={r}>
                  {status(r)}
                </p>
              ))}
            </>
          )}
        </Panel>
        <Panel title={l("2. Часы и сертификаты", "2. Clock and certificates")}>
          <p>
            {l(
              "На телефоне проверьте дату, время и часовой пояс. Ошибка часов может выглядеть как ошибка TLS. Не отключайте проверку сертификатов.",
              "Check date, time and timezone on the phone. Clock errors can look like TLS errors. Keep certificate validation enabled.",
            )}
          </p>
          <label>
            {l("Дата на телефоне", "Phone date")}
            <input
              type="date"
              value={clock}
              onChange={(e) => {
                setClock(e.target.value);
                setClockChecked(false);
              }}
            />
          </label>
          <button
            className="btn btn-secondary"
            disabled={!clock || busy || !progress}
            onClick={() =>
              void run(async () => {
                if (!progress) return;
                setProgress(
                  await api.call({
                    kind: "tls_clock",
                    fingerprint,
                    firmware: scope,
                    revision: progress.revision,
                    date: clock,
                  }),
                );
                setClockChecked(true);
              })
            }
          >
            {l("Сравнить с датой компьютера", "Compare with computer date")}
          </button>
          {clockChecked && (
            <p role="status">
              {clock ===
              new Intl.DateTimeFormat("en-CA", {
                year: "numeric",
                month: "2-digit",
                day: "2-digit",
              }).format(new Date())
                ? l(
                    "Дата совпадает; время и часовой пояс проверьте вручную.",
                    "Dates match; check time and timezone manually.",
                  )
                : l(
                    "Даты различаются — исправьте часы перед тестом.",
                    "Dates differ — correct the clock before testing.",
                  )}
            </p>
          )}
          <button className="btn btn-secondary" onClick={() => go("certificates")}>
            {l("Проверка сертификатов", "Inspect certificates")}
          </button>
        </Panel>
      </div>
      <Panel title={l("3. Закреплённые компоненты", "3. Pinned components")}>
        <div className="workbench-grid">
          {components.map((c) => (
            <section className="tls-component" key={c.id}>
              <h3>
                {c.spec.title[lang] ?? c.id} · {c.spec.version}
              </h3>
              <p>
                {c.spec.author} · {c.spec.released} · {c.spec.size.toLocaleString(lang)} B
              </p>
              <p className="mono">SHA-256 {c.spec.sha256}</p>
              <p>{c.spec.release_notes_text}</p>
              <details>
                <summary>{l("Источник и лицензия", "Source and licence")}</summary>
                <p>{c.spec.url}</p>
                <p>{c.spec.source_repository}</p>
                <pre className="contact-preview">{JSON.stringify(c.spec.licence, null, 2)}</pre>
              </details>
              <div className="btn-row">
                <button
                  className="btn btn-secondary"
                  disabled={busy}
                  onClick={() => void run(() => api.openAuthor(c.id))}
                >
                  {l("Открыть загрузку автора", "Open author download")}
                </button>
                <button
                  className="btn"
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      const path = await model.backend.pickFile();
                      if (path) {
                        await api.call({ kind: "tls_import", id: c.id, path });
                        await load();
                        setMessage(
                          l(
                            "Хеш, размер и структура проверены. Сохранено в локальном кэше.",
                            "Hash, size and structure verified. Saved to local cache.",
                          ),
                        );
                      }
                    })
                  }
                >
                  {l("Импортировать и проверить", "Import and validate")}
                </button>
              </div>
              <p className="tag">
                {c.cached
                  ? l("Подготовлен в кэше", "Staged in cache")
                  : l("Не загружен", "Not imported")}
              </p>
              {c.validation && (
                <p>
                  {c.validation.format.toUpperCase()} ·{" "}
                  {c.validation.signed === false
                    ? l("Без подписи; нужен InstallServer", "Unsigned; requires InstallServer")
                    : c.validation.files.join(", ")}
                </p>
              )}
              <p>
                {l("Передача", "Transfer")}:{" "}
                {(progress?.value.transferred[c.id]
                  ? status(progress.value.transferred[c.id].status)
                  : undefined) ?? l("Не выполнялась", "Not performed")}
              </p>
              <p>
                {l("Ручной отчёт", "Manual report")}:{" "}
                {(progress?.value.components[c.id]
                  ? status(progress.value.components[c.id].status)
                  : undefined) ?? l("Нет", "None")}
              </p>
              {c.id === "mbedtls-lib" && (
                <div className="btn-row">
                  <button
                    className="btn btn-secondary"
                    disabled={busy || !c.cached || !device || unsupported}
                    onClick={() => stage(c)}
                  >
                    {l("Передать SIS (не установка)", "Stage SIS (not installation)")}
                  </button>
                  <button
                    className="btn btn-secondary"
                    disabled={busy || !c.cached || unsupported}
                    onClick={() => record(c.id, "installed-reported")}
                  >
                    {l("Установку подтвердил на телефоне", "Installation confirmed on phone")}
                  </button>
                </div>
              )}
            </section>
          ))}
        </div>
      </Panel>
      <div className="workbench-grid">
        <Panel title={l("4. SSL DLL — ручной системный шаг", "4. SSL DLL — manual system step")}>
          <p>
            {l(
              "Автоприменение заблокировано: проверенные привилегированные права и безопасная замена системного файла на этой прошивке не подтверждены.",
              "Automatic apply is blocked: privileged rights and safe system replacement on this firmware are not verified.",
            )}
          </p>
          <ol>
            <li>
              {l(
                "Сначала создайте файловую копию доступных данных и отдельный снимок контактов.",
                "First back up accessible files and separately read a contacts snapshot.",
              )}
            </li>
            <li>
              {l(
                "В файловом менеджере с реальными правами Open4All сохраните исходный C:/sys/bin/ssl.dll. Если на C: его нет, запишите этот факт и сохраните исходный Z:/sys/bin/ssl.dll отдельно. Z: — только чтение.",
                "Using a phone file manager with confirmed Open4All rights, save the original C:/sys/bin/ssl.dll. If absent on C:, record that fact and separately preserve Z:/sys/bin/ssl.dll. Z: is read-only.",
              )}
            </li>
            <li>
              {l(
                "После установки MBedTLS вручную поместите проверенный ssl.dll в C:/sys/bin/ и перезагрузите телефон. Не заменяйте файл без исходной копии и плана восстановления.",
                "After installing MBedTLS, manually place the verified ssl.dll into C:/sys/bin/ and reboot. Keep the original and a recovery plan before replacement.",
              )}
            </li>
          </ol>
          <div className="btn-row">
            <button className="btn btn-secondary" onClick={() => go("backups")}>
              {l("Файловые копии", "File backups")}
            </button>
            <button className="btn btn-secondary" onClick={() => go("contacts")}>
              {l("Контакты", "Contacts")}
            </button>
            <button className="btn btn-secondary" onClick={() => go("files")}>
              {l("Файлы телефона", "Phone files")}
            </button>
          </div>
          <button
            className="btn btn-secondary"
            disabled={busy || unsupported}
            onClick={() => record("snapshot", "snapshot-reported")}
          >
            {l("Исходный DLL сохранён вручную", "Original DLL saved manually")}
          </button>
          <p>
            {l("Снимок", "Snapshot")}:{" "}
            {(progress?.value.snapshot ? status(progress.value.snapshot.status) : undefined) ??
              l("Не подтверждён", "Not reported")}
          </p>
          <button
            className="btn btn-secondary"
            disabled={
              busy || unsupported || !components.some((c) => c.id === "ssl-patch" && c.cached)
            }
            onClick={() => void run(async () => setDll(await api.call({ kind: "tls_extract" })))}
          >
            {l("Подготовить проверенный DLL локально", "Prepare verified DLL locally")}
          </button>
          {dll && <p className="path">{dll}</p>}
          <div className="btn-row">
            <button
              className="btn"
              disabled={busy || unsupported || !hasSnapshot || !sis || !dll}
              onClick={() => record("ssl-patch", "applied-reported")}
            >
              {l("DLL применён вручную", "DLL applied manually")}
            </button>
            <button
              className="btn btn-secondary"
              disabled={busy || !hasSnapshot}
              onClick={() => record("ssl-patch", "restored-reported")}
            >
              {l("Исходное состояние восстановлено вручную", "Original state restored manually")}
            </button>
          </div>
          <p className="hint">
            {l(
              "Откат — вернуть исходный C: файл либо удалить добавленный C: DLL, если его там не было. Затем перезагрузить и повторно проверить. Кэш ZIP не является резервной копией или откатом.",
              "To restore, return the original C: file or remove the added C: DLL if none existed there, then reboot and test again. The cached ZIP is neither a backup nor a rollback.",
            )}
          </p>
        </Panel>
        <Panel title={l("5. Проверка HTTPS на телефоне", "5. HTTPS test on the phone")}>
          <p>
            {l(
              "Откройте HTTPS-сайт в штатном браузере и в нужном приложении. Запишите, работает ли именно выбранный сетевой стек; Qt этим патчем не исправляется. Тест компьютера не проверяет TLS телефона.",
              "Open an HTTPS site in the phone browser and the target app. Test the selected network stack; this patch does not fix Qt. A host test cannot verify phone TLS.",
            )}
          </p>
          <p>
            {l("Результат", "Result")}:{" "}
            {(progress?.value.phone_https
              ? status(progress.value.phone_https.status)
              : undefined) ?? l("Не проверялось", "Not tested")}
          </p>
          <div className="btn-row">
            <button
              className="btn"
              disabled={busy || unsupported}
              onClick={() => record("phone-https", "passed-reported")}
            >
              {l("HTTPS работает (мой отчёт)", "HTTPS works (my report)")}
            </button>
            <button
              className="btn btn-secondary"
              disabled={busy}
              onClick={() => record("phone-https", "failed")}
            >
              {l("Ошибка на телефоне", "Phone test failed")}
            </button>
          </div>
          <p>
            {l(
              "При ошибке проверьте часы, сертификаты, установку MBedTLS, активные патчи и точную версию ОС. Изменение системного DLL может потребовать ручного восстановления; автоматических повторов нет.",
              "On failure, check the clock, certificates, MBedTLS installation, active patches and exact OS version. Changing a system DLL can require manual recovery; there are no automatic retries.",
            )}
          </p>
        </Panel>
      </div>
      {confirm && (
        <Dialog
          title={confirm.title}
          onClose={() => setConfirm(null)}
          actions={[
            { label: l("Отмена", "Cancel"), onClick: () => setConfirm(null) },
            {
              label: l("Подтвердить", "Confirm"),
              primary: true,
              onClick: () => {
                const fn = confirm.run;
                setConfirm(null);
                void run(fn);
              },
            },
          ]}
        >
          <p>{confirm.text}</p>
        </Dialog>
      )}
    </div>
  );
}
