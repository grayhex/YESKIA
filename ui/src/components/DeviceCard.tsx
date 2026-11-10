import { useI18n } from "../i18n";
import type { Device } from "../types";
import { PhoneIllustration } from "./PhoneIllustration";
import { ConnectionTag, MockTag } from "./Ui";

export function DeviceCard({ device, detailed = false }: { device: Device; detailed?: boolean }) {
  const { t } = useI18n();
  return (
    <div className="device-card">
      <PhoneIllustration model={device.identified_model ?? device.display_name} />
      <div>
        <h3 style={{ marginBottom: "0.25rem" }}>{device.display_name}</h3>
        <p style={{ margin: "0 0 0.5rem" }}>
          <MockTag origin={device.origin} /> <ConnectionTag state={device.connection} />
        </p>
        {detailed && (
          <dl className="facts">
            <dt>{t("dev.origin")}</dt>
            <dd>{device.origin}</dd>
            <dt>{t("dev.transport")}</dt>
            <dd>{device.transport}</dd>
            {device.serial && (
              <>
                <dt>{t("dev.serial")}</dt>
                <dd className="mono">{device.serial}</dd>
              </>
            )}
          </dl>
        )}
      </div>
    </div>
  );
}
