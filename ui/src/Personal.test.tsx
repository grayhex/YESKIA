import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockBackend } from "./mockBackend";

it("читает контакты, ищет и экспортирует только после выбора пути", async () => {
  const backend = new MockBackend();
  const read = vi.spyOn(backend, "readPersonalData").mockResolvedValue([
    { native_id: "1", name: "Анна", phones: ["123"] },
    { native_id: "2", name: "Борис", phones: ["456"] },
  ]);
  const save = vi.spyOn(backend, "savePersonalSnapshot").mockResolvedValue();
  vi.spyOn(backend, "pickFile")
    .mockResolvedValueOnce("C:\\python.exe")
    .mockResolvedValueOnce("C:\\gammu.ini");
  const pick = vi
    .spyOn(backend, "pickSavePath")
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce("C:\\contacts.json");
  render(<App backend={backend} />);
  await screen.findByText("Nokia N95 (MOCK)");
  await userEvent.click(
    within(screen.getByRole("navigation")).getByRole("button", { name: "Контакты" }),
  );
  expect(screen.getByRole("button", { name: /Прочитать телефон/ })).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Python…" }));
  await userEvent.click(screen.getByRole("button", { name: "Профиль…" }));
  await userEvent.click(screen.getByRole("button", { name: /Прочитать телефон/ }));
  expect(read).toHaveBeenCalledWith("C:\\python.exe", "C:\\gammu.ini", "contacts");
  await screen.findByRole("option", { name: /Анна/ });
  await userEvent.type(screen.getByRole("textbox", { name: "Поиск записей" }), "456");
  expect(screen.queryByRole("option", { name: /Анна/ })).not.toBeInTheDocument();
  expect(screen.getByRole("option", { name: /Борис/ })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: /Экспорт JSON/ }));
  expect(pick).toHaveBeenCalled();
  expect(save).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: /Экспорт JSON/ }));
  await waitFor(() =>
    expect(save).toHaveBeenCalledWith("C:\\contacts.json", expect.stringContaining('"Анна"')),
  );
});

it("каталог открыт по умолчанию, ищет карточки и не обещает установку", async () => {
  render(<App backend={new MockBackend()} />);
  await screen.findByText("Nokia N95 (MOCK)");
  await userEvent.click(
    within(screen.getByRole("navigation")).getByRole("button", { name: "Приложения" }),
  );
  await userEvent.type(screen.getByRole("textbox", { name: "Поиск приложений" }), "Opera Mini");
  expect(screen.getByRole("heading", { name: "Opera Mini (Java ME)" })).toBeInTheDocument();
  expect(screen.queryByText("Наборы приложений")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Установить/ })).not.toBeInTheDocument();
});
