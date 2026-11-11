// Development-only entry. Vite's production build includes only ui/index.html.
import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "../../src/App";
import { MockBackend } from "../../src/mockBackend";
import "../../src/styles/tokens.css";
import "../../src/styles/app.css";
import "../../src/styles/fluent.css";

const params = new URLSearchParams(location.search);
const scenario = params.get("scenario") ?? "connected";
class LayoutBackend extends MockBackend {
  async listDevices() {
    const devices = await super.listDevices();
    if (scenario !== "readonly") return devices;
    return devices.map((device) => ({
      ...device,
      transport: "mtp" as const,
      matrix: device.matrix && {
        ...device.matrix,
        entries: device.matrix.entries.map((entry) =>
          entry.operation === "files.write"
            ? { ...entry, availability: "unsupported" as const }
            : entry,
        ),
      },
    }));
  }
}
const backend = new LayoutBackend();
backend.settings.language = params.get("lang") === "en" ? "en" : "ru";
backend.settings.use_mock_device = scenario !== "empty";
backend.settings.onboarded = scenario !== "welcome";
if (["connected", "readonly", "long"].includes(scenario)) await backend.connectDevice("mock-e72");
if (scenario === "long") {
  for (let i = 0; i < 100; i++) {
    const name = `file-${i.toString().padStart(3, "0")}.jpg`;
    backend.deviceFiles[`E:/${name}`] = 4096;
    backend.hostFiles[`C:\\Users\\user\\Downloads\\${name}`] = 4096;
  }
}
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App backend={backend} />
  </React.StrictMode>,
);
