import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Backend } from "./api";
import { App } from "./App";
import { MockBackend } from "./mockBackend";
const nav = (name: string) => within(screen.getByRole("navigation")).getByRole("button", { name });
it("edits multiple contact values locally, requires explicit save and can undo the first save", async () => {
  const backend = new MockBackend();
  const phoneWrite = vi.spyOn(backend, "startOperation");
  render(<App backend={backend} />);
  await screen.findByText("Nokia E72 (MOCK)");
  await userEvent.click(nav("Контакты"));
  await userEvent.click(await screen.findByRole("button", { name: "Новый контакт" }));
  await userEvent.type(screen.getByRole("textbox", { name: "Отображаемое имя" }), "Локальный Иван");
  const fields = screen.getByRole("group", { name: "Телефоны" });
  await userEvent.click(within(fields).getByRole("button", { name: "Добавить поле" }));
  await userEvent.type(screen.getByRole("textbox", { name: "Телефоны 1" }), "+79990000000");
  await userEvent.click(within(fields).getByRole("button", { name: "Добавить поле" }));
  await userEvent.type(screen.getByRole("textbox", { name: "Телефоны 2" }), "+78880000000");
  expect(screen.queryByRole("option", { name: /Локальный Иван/ })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Сохранить локально" }));
  expect(await screen.findByRole("option", { name: /Локальный Иван/ })).toBeInTheDocument();
  expect(phoneWrite).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "Отменить последнее изменение" }));
  await waitFor(() =>
    expect(screen.queryByRole("option", { name: /Локальный Иван/ })).not.toBeInTheDocument(),
  );
});
it("keeps manual preparation behind confirmation, never offers a device-verified button and exports a private report", async () => {
  const backend = new MockBackend();
  const record = vi.spyOn(backend.workbench, "preparation");
  const save = vi.spyOn(backend as Backend, "savePersonalSnapshot").mockResolvedValue();
  vi.spyOn(backend, "pickSavePath").mockResolvedValue("C:\\readiness.json");
  render(<App backend={backend} />);
  await screen.findByText("Nokia E72 (MOCK)");
  await userEvent.click(screen.getByRole("button", { name: "Подготовка E72" }));
  expect(await screen.findByRole("heading", { name: "Подготовка E72" })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Ручная отметка" }));
  expect(record).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "record" }));
  await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Отмена" }));
  expect(record).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "record" }));
  await userEvent.click(screen.getByRole("button", { name: "Ручная отметка" }));
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", { name: "Сохранить отметку" }),
  );
  await waitFor(() =>
    expect(record).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: "record",
        step: "physical",
        status: "manual",
        expected_key: "mock-preparation",
      }),
    ),
  );
  expect(
    screen.queryByRole("button", { name: "Подтверждено устройством" }),
  ).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Экспорт отчёта готовности" }));
  await waitFor(() => expect(save).toHaveBeenCalled());
  const report = JSON.parse(save.mock.calls[0][1]);
  expect(report.ready).toBe(false);
  expect(report.steps.physical.status).toBe("manual");
  expect(report).not.toHaveProperty("context");
  expect(report).not.toHaveProperty("fingerprint");
});
it("opens pinned TLS components with explicit unsupported auto-apply and working deep links in English", async () => {
  render(<App backend={new MockBackend()} />);
  await screen.findByText("Nokia E72 (MOCK)");
  await userEvent.click(screen.getByRole("button", { name: "EN" }));
  await userEvent.click(nav("Certificates"));
  await userEvent.click(screen.getByRole("button", { name: "TLS / HTTPS Revival" }));
  expect(await screen.findByText(/926,820 B/)).toBeInTheDocument();
  expect(screen.getByText(/d20d6d41c796d53f/)).toBeInTheDocument();
  expect(screen.getByText(/Automatic apply is blocked/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Stage SIS (not installation)" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "DLL applied manually" })).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Prepare E72" }));
  expect(await screen.findByRole("heading", { name: "Prepare E72" })).toBeInTheDocument();
});
it("exports a TLS report only after choosing a path, keeping manual evidence distinct", async () => {
  const backend = new MockBackend();
  const save = vi.spyOn(backend as Backend, "savePersonalSnapshot").mockResolvedValue();
  vi.spyOn(backend, "pickSavePath")
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce("C:\\TLS-report.json");
  render(<App backend={backend} />);
  await screen.findByText("Nokia E72 (MOCK)");
  await userEvent.click(nav("Сертификаты"));
  await userEvent.click(screen.getByRole("button", { name: "TLS / HTTPS Revival" }));
  await userEvent.click(await screen.findByRole("button", { name: "Экспорт отчёта TLS" }));
  expect(save).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "Экспорт отчёта TLS" }));
  await waitFor(() =>
    expect(save).toHaveBeenCalledWith(
      "C:\\TLS-report.json",
      expect.stringContaining('"hardware_status": "not-tested; UI MOCK"'),
    ),
  );
});
