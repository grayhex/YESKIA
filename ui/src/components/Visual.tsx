import { createContext, useContext, type ReactNode } from "react";
import type { PageId } from "./Shell";
import { Icon, type IconName } from "./Icon";

export type AccentTone = "blue" | "green" | "purple" | "amber" | "red" | "neutral";
const themes: Record<PageId, { icon: IconName; tone: AccentTone }> = {
  preparation: { icon: "phone", tone: "green" },
  tls: { icon: "shield", tone: "blue" },
  agent: { icon: "activity", tone: "green" },
  home: { icon: "phone", tone: "blue" },
  connection: { icon: "usb", tone: "green" },
  doctor: { icon: "shield", tone: "green" },
  files: { icon: "folder", tone: "blue" },
  contacts: { icon: "contacts", tone: "amber" },
  sms: { icon: "message", tone: "amber" },
  calendar: { icon: "calendar", tone: "purple" },
  packages: { icon: "apps", tone: "purple" },
  studio: { icon: "music", tone: "amber" },
  backups: { icon: "archive", tone: "green" },
  certificates: { icon: "shield", tone: "blue" },
  journal: { icon: "activity", tone: "blue" },
  settings: { icon: "settings", tone: "neutral" },
};
const VisualContext = createContext(themes.home);
export function VisualProvider({ page, children }: { page: PageId; children: ReactNode }) {
  return <VisualContext.Provider value={themes[page]}>{children}</VisualContext.Provider>;
}
export const useVisualTheme = () => useContext(VisualContext);
export function AccentIcon({ name, tone = "blue" }: { name: IconName; tone?: AccentTone }) {
  return (
    <span className={`accent-icon accent-${tone}`} aria-hidden="true">
      <Icon name={name} />
    </span>
  );
}
