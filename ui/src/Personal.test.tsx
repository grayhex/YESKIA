import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockBackend } from "./mockBackend";
import { emptyContact } from "./workbench";

it("читает контакты, ищет и экспортирует только после выбора пути", async () => {
  const backend = new MockBackend();
  const book = {
    contacts: [
      {
        ...emptyContact(),
        name: "Анна",
        phones: [{ value: "123", label: "", params: [], components: [] }],
      },
      {
        ...emptyContact(),
        name: "Борис",
        phones: [{ value: "456", label: "", params: [], components: [] }],
      },
    ],
  };
  const read = vi
    .spyOn(backend.workbench, "phoneRead")
    .mockResolvedValue({
      book,
      previous: { contacts: [] },
      changes: [],
      hash: "a".repeat(64),
      source: "mock",
      read_at_ms: 1,
      write_status: "unsupported",
    });
  const call = vi.spyOn(backend.workbench, "call");
  const pick = vi
    .spyOn(backend, "pickSavePath")
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce("C:\\contacts.json");
  render(<App backend={backend} />);
  await screen.findByText("Nokia E72 (MOCK)");
  await userEvent.click(
    within(screen.getByRole("navigation")).getByRole("button", { name: "Контакты" }),
  );
  expect(screen.queryByText(/Python/)).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /Прочитать телефон/ })).toBeDisabled();
  await userEvent.click(
    within(screen.getByRole("navigation")).getByRole("button", { name: "Подключение" }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Подключить" }));
  await screen.findByRole("button", { name: "Отключить" });
  await userEvent.click(
    within(screen.getByRole("navigation")).getByRole("button", { name: "Контакты" }),
  );
  await userEvent.click(screen.getByRole("button", { name: /Прочитать телефон/ }));
  expect(read).toHaveBeenCalledWith(expect.any(String));
  await screen.findByRole("option", { name: /Анна/ });
  await userEvent.type(screen.getByRole("textbox", { name: "Поиск записей" }), "456");
  expect(screen.queryByRole("option", { name: /Анна/ })).not.toBeInTheDocument();
  expect(screen.getByRole("option", { name: /Борис/ })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: /Экспорт JSON/ }));
  expect(pick).toHaveBeenCalled();
  expect(call).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "contacts_export" }));
  await userEvent.click(screen.getByRole("button", { name: /Экспорт JSON/ }));
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "contacts_export", path: "C:\\contacts.json", book }),
    ),
  );
});

it("каталог открыт по умолчанию, ищет карточки и не обещает установку", async () => {
  render(<App backend={new MockBackend()} />);
  await screen.findByText("Nokia E72 (MOCK)");
  await userEvent.click(
    within(screen.getByRole("navigation")).getByRole("button", { name: "Приложения" }),
  );
  await userEvent.type(screen.getByRole("textbox", { name: "Поиск приложений" }), "Opera Mini");
  expect(screen.getByRole("heading", { name: "Opera Mini (Java ME)" })).toBeInTheDocument();
  expect(screen.queryByText("Наборы приложений")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Установить/ })).not.toBeInTheDocument();
});

it("переводит подключение и сертификаты на английский и требует подтверждения передачи", async () => {
  const backend = new MockBackend();
  vi.spyOn(backend, "pickFile").mockResolvedValue("C:\\root.cer");
  vi.spyOn(backend, "inspectCertificate").mockResolvedValue({
    subject: "CN=Test",
    issuer: "CN=Issuer",
    serial: "01",
    sha256: "a".repeat(64),
    not_before: "2025-01-01",
    not_after: "2030-01-01",
    validity: "valid",
    key_type: "RSA",
    self_issued: false,
  });
  const install = vi.spyOn(backend, "installPhoneCertificate").mockResolvedValue();
  render(<App backend={backend} />);
  await screen.findByText("Nokia E72 (MOCK)");
  const nav = within(screen.getByRole("navigation"));
  await userEvent.click(nav.getByRole("button", { name: "Подключение" }));
  await userEvent.click(screen.getByRole("button", { name: "Подключить" }));
  await screen.findByRole("button", { name: "Отключить" });
  await userEvent.click(screen.getByRole("button", { name: "EN" }));
  expect(await screen.findByRole("heading", { name: "Phone and connection" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Diagnostics" })).toBeInTheDocument();
  await userEvent.click(nav.getByRole("button", { name: "Certificates" }));
  await userEvent.click(screen.getByRole("button", { name: "Open certificate" }));
  expect(await screen.findByText("Issuer", { selector: "dt" })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Install on phone" }));
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByText(/SHA-256/)).toBeInTheDocument();
  expect(install).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(install).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "Install on phone" }));
  await userEvent.click(
    within(await screen.findByRole("dialog")).getByRole("button", { name: "Send to phone" }),
  );
  expect(install).toHaveBeenCalledWith(expect.any(String), "C:\\root.cer", "a".repeat(64), true);
  expect(await screen.findByText(/Certificate delivered/)).toBeInTheDocument();
});
