import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockBackend } from "./mockBackend";
import type { UpdateCheck } from "./types";

const nav = () => within(screen.getByRole("navigation"));

async function fresh(b = new MockBackend()) {
  b.settings = { ...b.settings, onboarded: false };
  render(<App backend={b} />);
  await screen.findByRole("heading", { name: "Добро пожаловать в YESKIA" });
  return b;
}

describe("Первый запуск", () => {
  it("приветствие объясняет границы и ничего не устанавливает", async () => {
    await fresh();
    expect(screen.getByTestId("wel-step")).toHaveTextContent("Шаг 1 из 4");
    expect(screen.getByText(/Не меняет и не заменяет драйверы/)).toBeInTheDocument();
    expect(screen.getByText(/серверных портов нет/)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Главная" })).not.toBeInTheDocument();
  });

  it("без телефона: понятное пустое состояние, помощь с драйвером без автоустановки", async () => {
    const b = new MockBackend();
    b.settings = { ...b.settings, use_mock_device: false };
    await fresh(b);
    await userEvent.click(screen.getByRole("button", { name: "Начать" }));
    await userEvent.click(screen.getByRole("button", { name: "Искать телефон" }));
    expect(await screen.findByText("Телефон не найден")).toBeInTheDocument();
    await userEvent.click(screen.getByText("Как установить драйвер"));
    expect(screen.getByText(/само драйверы не устанавливает и не заменяет/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Открыть диагностику" }));
    expect(
      await screen.findByRole("heading", { name: "Диагностика подключения" }),
    ).toBeInTheDocument();
    expect(b.settings.onboarded).toBe(true);
  });

  it("поиск → подключение → возможности (не проверено) → файловый менеджер", async () => {
    const b = await fresh();
    await userEvent.click(screen.getByRole("button", { name: "Начать" }));
    await userEvent.click(screen.getByRole("button", { name: "Искать телефон" }));
    await userEvent.click(await screen.findByRole("button", { name: /Подключить: Nokia N95/ }));
    await waitFor(() => expect(screen.getByTestId("wel-devices")).toHaveTextContent("Подключено"));
    await userEvent.click(screen.getByRole("button", { name: "Далее" }));
    const caps = await screen.findByTestId("wel-caps");
    expect(within(caps).getAllByText("не поддерживается").length).toBeGreaterThan(0);
    expect(within(caps).getAllByText("не проверено").length).toBeGreaterThan(0);
    await userEvent.click(screen.getByRole("button", { name: "Далее" }));
    await userEvent.click(screen.getByRole("button", { name: "Открыть файловый менеджер" }));
    expect(await screen.findByRole("heading", { name: "Файловый менеджер" })).toBeInTheDocument();
    expect(b.settings.onboarded).toBe(true);
  });

  it("можно пропустить, и приветствие больше не возвращается", async () => {
    const b = await fresh();
    await userEvent.click(screen.getByRole("button", { name: "Пропустить приветствие" }));
    expect(await screen.findByRole("heading", { name: "Главная" })).toBeInTheDocument();
    expect(b.settings.onboarded).toBe(true);
    await userEvent.click(nav().getByRole("button", { name: "Настройки" }));
    expect(await screen.findByRole("heading", { name: "Настройки" })).toBeInTheDocument();
  });

  it("на шаге файлового менеджера без доступа к файлам кнопка неактивна и причина названа", async () => {
    await fresh();
    await userEvent.click(screen.getByRole("button", { name: "Начать" }));
    await userEvent.click(screen.getByRole("button", { name: "Далее" }));
    expect(screen.getByText("Телефон не подключён")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Далее" }));
    expect(screen.getByRole("button", { name: "Открыть файловый менеджер" })).toBeDisabled();
    expect(screen.getByText(/недоступны/)).toBeInTheDocument();
  });
});

const unsigned: UpdateCheck = {
  current: "0.1.0",
  version: "0.2.0",
  notes: "Исправления",
  installer: "YESKIA_0.2.0_x64-setup.exe",
  size: 12_345_678,
  sha256: "a".repeat(64),
  trust: "unsigned-source",
  limitations: ["Приложение ничего не скачивает: установщик получен вами самостоятельно."],
};

async function settings(b: MockBackend) {
  render(<App backend={b} />);
  await screen.findByText("Nokia N95 (MOCK)");
  await userEvent.click(nav().getByRole("button", { name: "Настройки" }));
  await screen.findByRole("heading", { name: "Обновление приложения" });
}

async function pickAndVerify(b: MockBackend) {
  b.nextMediaFile = "C:\\Users\\user\\Downloads\\YESKIA_0.2.0_x64-setup.exe";
  await userEvent.click(screen.getByRole("button", { name: "Выбрать установщик…" }));
  b.nextMediaFile = "C:\\Users\\user\\Downloads\\update.json";
  await userEvent.click(screen.getByRole("button", { name: "Выбрать манифест…" }));
  await userEvent.click(screen.getByRole("button", { name: "Проверить" }));
}

describe("Обновление из файла", () => {
  it("режим и каталог данных видны; обновление объясняет, что ничего не скачивается", async () => {
    await settings(new MockBackend());
    expect(screen.getByTestId("settings-mode")).toHaveTextContent("Установленный");
    expect(screen.getByText(/ничего не скачивает/, { selector: "p.banner" })).toBeInTheDocument();
    expect(screen.getByText(/подпись издателя пока не настроена/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Проверить" })).toBeDisabled();
  });

  it("неподписанный источник: ограничения видны, запуск требует отдельного согласия", async () => {
    const b = new MockBackend();
    b.updateSample = unsigned;
    await settings(b);
    await pickAndVerify(b);
    expect(await screen.findByTestId("up-result")).toHaveTextContent("Версия 0.2.0 (сейчас 0.1.0)");
    expect(screen.getByTestId("up-trust")).toHaveTextContent("Подпись издателя не проверена");
    await userEvent.click(screen.getByRole("button", { name: "Запустить установщик…" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Запустить" }));
    expect(b.launched).toEqual([]);
    await userEvent.click(within(dialog).getByRole("checkbox"));
    await userEvent.click(within(dialog).getByRole("button", { name: "Запустить" }));
    await waitFor(() => expect(b.launched).toHaveLength(1));
    expect(screen.getByText(/Установщик запущен/)).toBeInTheDocument();
  });

  it("подписанный источник запускается без согласия на неподписанное", async () => {
    const b = new MockBackend();
    b.updateSample = { ...unsigned, trust: "signed" };
    await settings(b);
    await pickAndVerify(b);
    expect(await screen.findByTestId("up-trust")).toHaveTextContent("проверена доверенным ключом");
    await userEvent.click(screen.getByRole("button", { name: "Запустить установщик…" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByRole("checkbox")).not.toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Запустить" }));
    await waitFor(() => expect(b.launched).toHaveLength(1));
  });

  it("при записи на телефон обновление откладывается", async () => {
    const b = new MockBackend();
    b.updateSample = unsigned;
    b.updateBlockedBy = ["device-operation"];
    await settings(b);
    await pickAndVerify(b);
    expect(await screen.findByTestId("up-blocked")).toHaveTextContent("отложено");
    expect(screen.getByRole("button", { name: "Запустить установщик…" })).toBeDisabled();
    expect(b.launched).toEqual([]);
  });

  it("ошибка проверки (подмена файла) показывается и запуск недоступен", async () => {
    const b = new MockBackend();
    b.updateSample = null;
    await settings(b);
    await pickAndVerify(b);
    expect(await screen.findByTestId("error")).toHaveTextContent("VERIFICATION_FAILED");
    expect(screen.queryByRole("button", { name: "Запустить установщик…" })).not.toBeInTheDocument();
  });
});
