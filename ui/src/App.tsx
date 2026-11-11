import { useEffect, useRef, useState } from "react";
import type { Backend } from "./api";
import { VisualProvider } from "./components/Visual";
import { Icon } from "./components/Icon";
import { ErrorMessage } from "./components/Ui";
import { Header, SideNav, StatusBar, type NavEntry, type PageId } from "./components/Shell";
import { I18nProvider, makeTranslate, normalizeLang } from "./i18n";
import { AgentPage } from "./pages/AgentPage";
import { StorePage } from "./pages/StorePage";
import { DevicePage } from "./pages/DevicePage";
import { ContactsPage } from "./pages/ContactsPage";
import { PreparationPage } from "./pages/PreparationPage";
import { TlsPage } from "./pages/TlsPage";
import { PersonalPage } from "./pages/PersonalPage";
import { CertificatesPage } from "./pages/CertificatesPage";
import { StudioPage } from "./pages/StudioPage";
import { BackupsPage } from "./pages/BackupsPage";
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
    { id: "agent", label: "nav.agent" },
    { id: "files", label: "nav.files" },
    { id: "contacts", label: "nav.contacts" },
    { id: "sms", label: "nav.sms" },
    { id: "calendar", label: "nav.calendar" },
    { id: "packages", label: "nav.packages" },
    { id: "studio", label: "nav.studio" },
    { id: "certificates", label: "nav.certificates" },
    { id: "backups", label: "nav.backups" },
    ...(developer ? ([{ id: "journal", label: "nav.journal" }] as NavEntry[]) : []),
    { id: "doctor", label: "nav.help" },
    { id: "settings", label: "nav.settings" },
  ];
  // Страница журнала доступна только в режиме «Разработчик».
  const current: PageId =
    page === "doctor" ? "connection" : page === "journal" && !developer ? "home" : page;
  const currentLabel =
    current === "preparation"
      ? "nav.preparation"
      : current === "tls"
        ? "nav.tls"
        : nav.find((n) => n.id === current)!.label;

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
        <SideNav
          items={nav}
          current={
            current === "preparation" ? "home" : current === "tls" ? "certificates" : current
          }
          onSelect={select}
          device={connected ?? model.devices[0]}
        />
        <div className="workspace-bar">
          <div className="workspace-crumb">
            <span>{t("nav.group.workspace")}</span>
            <span aria-hidden="true">›</span>
            <strong>{t(currentLabel)}</strong>
          </div>
          <button
            className="btn btn-secondary"
            disabled={model.busy}
            onClick={() => void model.refresh()}
          >
            <Icon name="refresh" />
            {t("fm.refresh")}
          </button>
        </div>
        <VisualProvider page={current}>
          <main data-page={current} id="main" className="content" tabIndex={-1} ref={mainRef}>
            {model.isUiMock && (
              <p role="alert" className="banner banner-warn">
                {t("banner.uiMock")}
              </p>
            )}
            {model.error && <ErrorMessage error={model.error} developer={developer} />}
            {welcome && (
              <WelcomePage
                model={model}
                go={select}
                onFinish={() => void model.updateSettings({ onboarded: true })}
              />
            )}
            {!welcome && current === "home" && <HomePage model={model} go={select} />}
            {!welcome && current === "connection" && (
              <DevicePage
                key={page}
                diagnostic={page === "doctor"}
                model={model}
                developer={developer}
                go={select}
              />
            )}
            {!welcome && current === "agent" && <AgentPage model={model} />}
            {!welcome && current === "files" && <FileManagerPage model={model} go={select} />}
            {!welcome && current === "packages" && <StorePage model={model} />}
            {!welcome && current === "studio" && <StudioPage model={model} />}
            {!welcome && current === "backups" && <BackupsPage model={model} />}
            {!welcome && (current === "sms" || current === "calendar") && (
              <PersonalPage key={current} domain={current} model={model} />
            )}
            {!welcome && current === "contacts" && <ContactsPage model={model} />}
            {!welcome && current === "preparation" && <PreparationPage model={model} go={select} />}
            {!welcome && current === "tls" && <TlsPage model={model} go={select} />}
            {!welcome && current === "certificates" && (
              <CertificatesPage model={model} go={select} />
            )}
            {!welcome && current === "journal" && <JournalPage model={model} />}
            {!welcome && current === "settings" && <SettingsPage model={model} />}
          </main>
        </VisualProvider>
        <StatusBar>
          <span>{t("status.offline")}</span>
          <span>{t("status.devices", { n: model.devices.length })}</span>
          <span>
            {connected
              ? t("status.connected", { name: connected.display_name })
              : t("status.noDevice")}
          </span>
          {model.settings?.use_mock_device && <span>{t("status.mockOn")}</span>}
          {developer && model.isUiMock && <span>{t("status.uiMock")}</span>}
        </StatusBar>
      </div>
    </I18nProvider>
  );
}
