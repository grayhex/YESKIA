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
  async agentSetLab(id:string,armed:boolean,acknowledge:boolean) {
    if(scenario!=="agent") return super.agentSetLab(id,armed,acknowledge);
    if(armed && !acknowledge) throw new Error("synthetic Lab profile needs consent");
  }
  async listDevices() {
    const devices = await super.listDevices();
    if(scenario==="multiple") return devices.flatMap(device=>["bluetooth","pc_suite","mtp","mass_storage"].map((transport,index)=>({...device,id:`synthetic-route-${index}`,display_name:`Nokia E72 · route ${index+1} (MOCK)`,transport:transport as typeof device.transport})));
    if (scenario === "agent") return devices.map(device => ({...device,id:"adbian-synthetic-layout",display_name:"Synthetic E72 · preview",transport:"bluetooth" as const,identified_model:null}));
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
backend.settings.theme=params.get("theme")==="dark"?"dark":"light";
backend.settings.language = params.get("lang") === "en" ? "en" : "ru";
backend.settings.use_mock_device = scenario !== "empty";
backend.settings.onboarded = scenario !== "welcome";
backend.settings.developer_mode = ["agent","multiple"].includes(scenario);
if (["connected", "readonly", "long", "agent","multiple"].includes(scenario)) await backend.connectDevice("mock-e72");
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
