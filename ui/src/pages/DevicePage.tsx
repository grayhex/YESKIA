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
  const [doctor, setDoctor] = useState(diagnostic);
  return (
    <div className="workspace-page device-workspace">
      <div className="page-heading">
        <h1>Телефон и подключение</h1>
        <div className="btn-row">
          <button
            className="btn btn-secondary"
            aria-pressed={!doctor}
            onClick={() => setDoctor(false)}
          >
            Подключение
          </button>
          <button
            className="btn btn-secondary"
            aria-pressed={doctor}
            onClick={() => setDoctor(true)}
          >
            Диагностика
          </button>
        </div>
      </div>
      <div className="workspace-scroll">
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
