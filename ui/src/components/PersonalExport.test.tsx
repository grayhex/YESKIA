import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockBackend } from "../mockBackend";
import { I18nProvider } from "../i18n";
import { PersonalExport } from "./PersonalExport";

it("выбирает Gammu и конфигурацию, затем экспортирует контакты в выбранный файл", async () => {
  const backend = new MockBackend();
  vi.spyOn(backend, "pickFile")
    .mockResolvedValueOnce("C:\\gammu.exe")
    .mockResolvedValueOnce("C:\\gammurc");
  vi.spyOn(backend, "pickSavePath").mockResolvedValue("C:\\contacts.vcf");
  const exportData = vi.spyOn(backend, "exportPersonalData").mockResolvedValue();
  render(
    <I18nProvider lang="ru">
      <PersonalExport backend={backend} />
    </I18nProvider>,
  );
  await userEvent.click(screen.getByRole("button", { name: "Выбрать Gammu" }));
  await userEvent.click(screen.getByRole("button", { name: "Выбрать настройки Gammu" }));
  await userEvent.click(screen.getByRole("button", { name: "Сохранить контакты" }));
  await waitFor(() =>
    expect(exportData).toHaveBeenCalledWith(
      "C:\\gammu.exe",
      "C:\\gammurc",
      "C:\\contacts.vcf",
      "contacts",
    ),
  );
  expect(await screen.findByText(/Файл сохранён/)).toBeInTheDocument();
});

it("отмена диалога не запускает чтение телефона", async () => {
  const backend = new MockBackend();
  vi.spyOn(backend, "pickFile")
    .mockResolvedValueOnce("C:\\gammu.exe")
    .mockResolvedValueOnce("C:\\gammurc");
  vi.spyOn(backend, "pickSavePath").mockResolvedValue(null);
  const exportData = vi.spyOn(backend, "exportPersonalData");
  render(
    <I18nProvider lang="ru">
      <PersonalExport backend={backend} />
    </I18nProvider>,
  );
  await userEvent.click(screen.getByRole("button", { name: "Выбрать Gammu" }));
  await userEvent.click(screen.getByRole("button", { name: "Выбрать настройки Gammu" }));
  await userEvent.click(screen.getByRole("button", { name: "Сохранить календарь" }));
  expect(exportData).not.toHaveBeenCalled();
});
