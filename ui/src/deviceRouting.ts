import type { Device, Operation } from "./types";
/** Choose a route for one operation; never merge identities based on model/paired name. */
export function selectDeviceForOperation(
  devices: Device[],
  operation: Operation,
  chosen?: string,
): Device | undefined {
  const connected = devices.filter((d) => d.connection === "connected");
  const explicit = connected.find((d) => d.id === chosen);
  if (explicit) return explicit;
  const files = operation.startsWith("files.") || operation === "packages.transfer";
  return connected
    .map((device, index) => {
      const entry = device.matrix?.entries.find((e) => e.operation === operation);
      const supported =
        entry && entry.availability !== "unsupported" && entry.maturity !== "unsupported";
      const preferred = files ? device.id.startsWith("adbian-") : device.transport !== "bluetooth";
      return { device, score: (supported ? 10 : 0) + (preferred ? 2 : 0), index };
    })
    .sort((a, b) => b.score - a.score || a.index - b.index)[0]?.device;
}
