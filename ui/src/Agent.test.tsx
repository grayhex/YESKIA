import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockBackend } from "./mockBackend";
import type { AgentPackage, Device, Operation, OperationRequest, OperationResult } from "./types";

class AgentBackend extends MockBackend {
  requests: OperationRequest[] = [];
  arming: [string, boolean, boolean][] = [];
  armed = false;
  fail = false;
  devices: Device[] = [
    {
      id: "adbian-synthetic",
      display_name: "Synthetic E72",
      origin: "real",
      transport: "bluetooth",
      connection: "connected",
      serial: null,
      identified_model: null,
      matrix: {
        device_fingerprint: "synthetic",
        probed_at_ms: 1,
        entries: (
          [
            "files.list",
            "files.read",
            "files.write",
            "files.mkdir",
            "packages.transfer",
            "system.read",
            "process.exec",
            "device.reset",
            "diagnostics.read",
          ] as Operation[]
        ).map((operation) => ({
          operation,
          availability: "not-tested",
          maturity: "implemented",
          detail: null,
          evidence: [],
        })),
      },
    },
  ];
  async listDevices() {
    return structuredClone(this.devices);
  }
  async agentSetLab(id: string, armed: boolean, acknowledge: boolean) {
    this.arming.push([id, armed, acknowledge]);
    if (armed && !acknowledge) throw new Error("not armed");
    this.armed = armed;
  }
  async startOperation(_id: string, request: OperationRequest) {
    this.requests.push(request);
    return String(this.requests.length - 1);
  }
  async awaitOperation(operationId: string): Promise<OperationResult> {
    const req = this.requests[Number(operationId)];
    return {
      operation_id: operationId,
      device_id: this.devices[0].id,
      operation: req.type === "agent.ping" ? "diagnostics.read" : "system.read",
      origin: "real",
      outcome: this.fail ? "failed" : "success",
      error: this.fail
        ? {
            code: "DEVICE_DISCONNECTED",
            message: "Disconnected",
            retryable: true,
            action: "verify_device_contents",
          }
        : null,
      uncertain: this.fail,
      data: this.fail
        ? null
        : req.type === "agent.command" || req.type === "agent.ping"
          ? {
              type: "agent_reply",
              reply: {
                ok: true,
                text: req.type === "agent.ping" ? "pong" : "synthetic reply",
                data: [],
              },
            }
          : { type: "directory_created" },
      duration_ms: 1,
      evidence: [],
    };
  }
  async pickPackageFiles() {
    return ["C:\\synthetic\\agent.sis"];
  }
  async agentPackageInfo(): Promise<AgentPackage> {
    return { name: "agent.sis", bytes: 512, sha256: "a".repeat(64) };
  }
}
async function open(developer = true) {
  const backend = new AgentBackend();
  await backend.saveSettings({ ...(await backend.getSettings()), developer_mode: developer });
  render(<App backend={backend} />);
  await userEvent.click(
    within(screen.getByRole("navigation")).getByRole("button", { name: "Агент на телефоне" }),
  );
  await screen.findByRole("heading", { name: "Агент на телефоне" });
  if (developer) await userEvent.click(screen.getByRole("button", { name: "Device Lab" }));
  return backend;
}
async function arm() {
  await userEvent.click(screen.getByRole("checkbox", { name: /Это моя экспериментальная E72/ }));
  await userEvent.click(screen.getByRole("button", { name: "Включить Lab для этого телефона" }));
  await screen.findByRole("button", { name: "Выключить Lab" });
}
function commandInput() {
  return screen.getByRole("textbox", { name: "Команда на телефоне" });
}
it("keeps Lab commands out of normal mode and does not infer a version from pong", async () => {
  const backend = await open(false);
  expect(screen.queryByText("Device Lab / ADBian Console")).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Проверить ответ" }));
  expect(await screen.findByText(/Версия базового ADBian неизвестна/)).toBeInTheDocument();
  expect(backend.requests).toEqual([{ type: "agent.ping" }]);
});
it("arms explicitly and confirms the exact destructive command and selected phone", async () => {
  const backend = await open();
  expect(commandInput()).toBeDisabled();
  expect(screen.getByRole("button", { name: "Включить Lab для этого телефона" })).toBeDisabled();
  await arm();
  await userEvent.clear(commandInput());
  await userEvent.type(commandInput(), "rm C:\\Data\\test.txt");
  await userEvent.click(screen.getByRole("button", { name: "Выполнить" }));
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByText("Synthetic E72")).toBeInTheDocument();
  expect(within(dialog).getByText("rm C:\\Data\\test.txt")).toBeInTheDocument();
  expect(backend.requests).toHaveLength(0);
  await userEvent.click(within(dialog).getByRole("button", { name: "Отмена" }));
  expect(backend.requests).toHaveLength(0);
  await userEvent.click(screen.getByRole("button", { name: "Выполнить" }));
  await userEvent.click(screen.getByRole("button", { name: "Запустить на этом телефоне" }));
  await screen.findByText(/synthetic reply/);
  expect(backend.requests).toEqual([
    { type: "agent.command", line: "rm C:\\Data\\test.txt", confirm: true },
  ]);
});
it("stages a package with a hash and reports transfer separately from installation", async () => {
  const backend = await open(false);
  await userEvent.click(screen.getByRole("button", { name: "Sideload SIS / JAR" }));
  expect(await screen.findByRole("dialog")).toBeInTheDocument();
  expect(screen.getByText(`SHA-256 ${"a".repeat(64)}`)).toBeInTheDocument();
  expect(backend.requests).toHaveLength(0);
  await userEvent.click(screen.getByRole("button", { name: "Запустить на этом телефоне" }));
  await screen.findByText("Готово к установке: agent.sis");
  expect(backend.requests.map((r) => r.type)).toEqual(["files.mkdir", "packages.transfer"]);
  expect(screen.getByRole("checkbox", { name: /Я завершил установку/ })).not.toBeChecked();
  expect(backend.armed).toBe(false);
});
it("reports uncertain outcomes without replay and disarms when leaving the page", async () => {
  const backend = await open();
  await arm();
  backend.fail = true;
  await userEvent.clear(commandInput());
  await userEvent.type(commandInput(), "exec C:\\sys\\bin\\test.exe");
  await userEvent.click(screen.getByRole("button", { name: "Выполнить" }));
  await userEvent.click(screen.getByRole("button", { name: "Запустить на этом телефоне" }));
  await screen.findByText(/Проверьте телефон перед ручным повтором/);
  expect(backend.requests).toHaveLength(1);
  await userEvent.click(
    within(screen.getByRole("navigation")).getByRole("button", { name: "Главная" }),
  );
  await waitFor(() => expect(backend.armed).toBe(false));
});
it("confirms the versioned helper invocation instead of sending invented shell verbs", async () => {
  const backend = await open();
  await arm();
  await userEvent.click(screen.getByRole("button", { name: "YESKIA capabilities" }));
  const d = await screen.findByRole("dialog");
  expect(within(d).getByText(/exec C:\/sys\/bin\/yeskiaagent.exe/)).toBeInTheDocument();
  await userEvent.click(within(d).getByRole("button", { name: "Запустить на этом телефоне" }));
  await waitFor(() => expect(backend.requests).toHaveLength(1));
  expect(backend.requests[0]).toEqual({
    type: "agent.query",
    query: { command: "capabilities" },
    confirm: true,
  });
});
