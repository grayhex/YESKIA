import { selectDeviceForOperation } from "./deviceRouting";
import type { Device, Operation } from "./types";
function device(id: string, transport: Device["transport"], operations: Operation[]): Device {
  return {
    id,
    transport,
    display_name: "same paired name",
    origin: "real",
    connection: "connected",
    serial: null,
    identified_model: null,
    matrix: {
      device_fingerprint: id,
      probed_at_ms: 1,
      entries: operations.map((operation) => ({
        operation,
        availability: "not-tested",
        maturity: "implemented",
        detail: null,
        evidence: [],
      })),
    },
  };
}
it("routes files to agent and personal data to USB independently, preserving explicit selection", () => {
  const usb = device("usb", "pc_suite", ["contacts.read", "files.list"]);
  const agent = device("adbian-lab", "bluetooth", ["files.list"]);
  expect(selectDeviceForOperation([usb, agent], "files.list")?.id).toBe(agent.id);
  expect(selectDeviceForOperation([agent, usb], "contacts.read")?.id).toBe(usb.id);
  expect(selectDeviceForOperation([usb, agent], "files.list", usb.id)?.id).toBe(usb.id);
  expect(
    selectDeviceForOperation([{ ...agent, connection: "disconnected" }, usb], "files.list")?.id,
  ).toBe(usb.id);
});
