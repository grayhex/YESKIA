import { useEffect, useRef, useState } from "react";
import type { Backend } from "./api";
import { ErrorMessage } from "./components/Ui";
import { Header, SideNav, StatusBar, type NavEntry, type PageId } from "./components/Shell";
import { I18nProvider, makeTranslate, normalizeLang } from "./i18n";
import { PackagesPage } from "./pages/PackagesPage";
import { StudioPage } from "./pages/StudioPage";
import { BackupsPage } from "./pages/BackupsPage";
import { ConnectionPage } from "./pages/ConnectionPage";
import { DoctorPage } from "./pages/DoctorPage";
import { FileManagerPage } from "./pages/FileManagerPage";
import { HomePage } from "./pages/HomePage";
import { JournalPage } from "./pages/JournalPage";
import { SettingsPage } from "./pages/SettingsPage";
import { WelcomePage } from "./pages/WelcomePage";
import { useAppModel } from "./useAppModel";

export function App({ backend }: { backend: Backend }) {
  const model = useAppModel(backend);
  const [page, setPage] = useState<PageId>("home");
  const mainRef = useRef<HTMLElement>(null);

  const lang = normalizeLang(model.settings?.language);
  const developer = model.settings?.developer_mode ?? false;
  const t = makeTranslate(lang);

  const nav: NavEntry[] = [
    { id: "home", label: "nav.home" },
    { id: "connection", label: "nav.connection" },
    { id: "files", label: "nav.files" },
    { id: "packages", label: "nav.packages" },
    { id: "studio", label: "nav.studio" },
    { id: "backups", label: "nav.backups" },
    { id: "doctor", label: "nav.doctor" },
    ...(developer ? ([{ id: "journal", label: "nav.journal" }] as NavEntry[]) : []),
    { id: "settings", label: "nav.settings" },
  ];
  // Страница журнала доступна только в режиме «Разработчик».
  const current: PageId = page === "journal" && !developer ? "home" : page;
  const currentLabel = nav.find((n) => n.id === current)!.label;

  useEffect(() => {
    document.documentElement.lang = lang;
    document.title = `${t(currentLabel)} — YESKIA`;
  });

  function select(id: PageId) {
    setPage(id);
    mainRef.current?.focus();
  }

  const connected = model.devices.find((d) => d.connection === "connected");
  // Первый запуск: приветствие вместо обычных страниц, пока пользователь его не закроет.
  const welcome = !!model.settings && !model.settings.onboarded;

  return (
    <I18nProvider lang={lang}>
      <a
        className="skip-link"
        href="#main"
        onClick={(e) => (e.preventDefault(), mainRef.current?.focus())}
      >
        {t("skipToContent")}
      </a>
      <div className="shell">
        <Header
          developer={developer}
          lang={lang}
          disabled={!model.settings || model.busy}
          onMode={(d) => model.updateSettings({ developer_mode: d })}
          onLang={(l) => model.updateSettings({ language: l })}
        />
        <SideNav items={nav} current={current} onSelect={select} />
        <main id="main" className="content" tabIndex={-1} ref={mainRef}>
          {model.isUiMock && (
            <p role="alert" className="banner banner-warn">
              {t("banner.uiMock")}
            </p>
          )}
          {model.error && <ErrorMessage error={model.error} />}
          {welcome && (
            <WelcomePage
              model={model}
              go={select}
              onFinish={() => void model.updateSettings({ onboarded: true })}
            />
          )}
          {!welcome && current === "home" && <HomePage model={model} go={select} />}
          {!welcome && current === "connection" && (
            <ConnectionPage model={model} developer={developer} />
          )}
          {!welcome && current === "files" && <FileManagerPage model={model} go={select} />}
          {!welcome && current === "packages" && <PackagesPage model={model} />}
          {!welcome && current === "studio" && <StudioPage model={model} />}
          {!welcome && current === "backups" && <BackupsPage model={model} />}
          {!welcome && current === "doctor" && <DoctorPage model={model} developer={developer} />}
          {!welcome && current === "journal" && <JournalPage model={model} />}
          {!welcome && current === "settings" && <SettingsPage model={model} />}
        </main>
        <StatusBar>
          <span>{t("status.offline")}</span>
          <span>{t("status.devices", { n: model.devices.length })}</span>
          <span>
            {connected
              ? t("status.connected", { name: connected.display_name })
              : t("status.noDevice")}
          </span>
          {model.settings?.use_mock_device && <span>{t("status.mockOn")}</span>}
          {model.isUiMock && <span>{t("status.uiMock")}</span>}
        </StatusBar>
      </div>
    </I18nProvider>
  );
}
