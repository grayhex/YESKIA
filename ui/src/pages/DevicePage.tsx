import { useWorkspaceCopy } from "../workspaceCopy";
import { useState } from "react";
import { ConnectionPage } from "./ConnectionPage";
import { DoctorPage } from "./DoctorPage";
import type { AppModel } from "../useAppModel";
import type { PageId } from "../components/Shell";
export function DevicePage({
  model,
  developer,
  go,
  diagnostic = false,
}: {
  model: AppModel;
  developer: boolean;
  go: (p: PageId) => void;
  diagnostic?: boolean;
}) {
  const { c } = useWorkspaceCopy();
  const [doctor, setDoctor] = useState(diagnostic);
  return (
    <div className="workspace-page device-workspace">
      <div className="page-heading">
        <h1>{c("device.title")}</h1>
        <div className="btn-row">
          <button
            className="btn btn-secondary"
            aria-pressed={!doctor}
            onClick={() => setDoctor(false)}
          >
            {c("device.connection")}
          </button>
          <button
            className="btn btn-secondary"
            aria-pressed={doctor}
            onClick={() => setDoctor(true)}
          >
            {c("device.diagnostics")}
          </button>
        </div>
      </div>
      <div className={`workspace-scroll ${doctor ? "" : "connection-workspace"}`}>
        {doctor ? (
          <DoctorPage model={model} developer={developer} />
        ) : (
          <ConnectionPage
            model={model}
            developer={developer}
            go={(p) => (p === "doctor" ? setDoctor(true) : go(p))}
          />
        )}
      </div>
    </div>
  );
}
