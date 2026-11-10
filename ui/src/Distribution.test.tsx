import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockBackend } from "./mockBackend";

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
    expect(
      screen.getByText(/Данные телефона обрабатываются на этом компьютере/),
    ).toBeInTheDocument();
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

it("открывает страницу последнего выпуска", async () => {
  const backend = new MockBackend();
  const open = vi.spyOn(backend, "openLatestRelease");
  render(<App backend={backend} />);
  await screen.findByText("Nokia N95 (MOCK)");
  await userEvent.click(nav().getByRole("button", { name: "Настройки" }));
  await userEvent.click(await screen.findByRole("button", { name: "Последний выпуск" }));
  expect(open).toHaveBeenCalledOnce();
});
