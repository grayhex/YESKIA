import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockBackend } from "./mockBackend";
import { App } from "./App";

async function open(backend = new MockBackend()) {
  render(<App backend={backend} />);
  await screen.findByText("Nokia N95 (MOCK)");
  return backend;
}

const nav = () => within(screen.getByRole("navigation"));

async function goConnection() {
  await userEvent.click(nav().getByRole("button", { name: "Подключение" }));
  await screen.findByRole("heading", { name: "Подключение" });
}

describe("Главная", () => {
  it("помечает UI-заглушку и mock-устройство, цифры демо-данных тоже", async () => {
    await open();
    expect(screen.getByText(/UI-заглушка: данные не от Rust-ядра/)).toBeInTheDocument();
    expect(screen.getAllByText("MOCK").length).toBeGreaterThan(0);
    expect(screen.getByText("демо-данные")).toBeInTheDocument();
  });

  it("показывает пустое состояние без устройств и ведёт в «Подключение»", async () => {
    const backend = new MockBackend();
    await backend.saveSettings({ ...(await backend.getSettings()), use_mock_device: false });
    render(<App backend={backend} />);
    expect(await screen.findByText("Устройство не выбрано")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Открыть раздел/ }));
    expect(await screen.findByText("Устройства не найдены.")).toBeInTheDocument();
  });

  it("не использует hash-маршрутизацию", async () => {
    await open();
    await goConnection();
    expect(window.location.hash).toBe("");
  });
});

describe("Навигация и доступность", () => {
  it("переключается стрелками с клавиатуры и помечает текущую страницу", async () => {
    await open();
    const home = nav().getByRole("button", { name: "Главная" });
    expect(home).toHaveAttribute("aria-current", "page");
    home.focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(nav().getByRole("button", { name: "Подключение" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: "Подключение" })).toBeInTheDocument();
    expect(document.title).toContain("Подключение");
  });

  it("есть ссылка «Перейти к содержимому»", async () => {
    await open();
    expect(screen.getByText("Перейти к содержимому")).toBeInTheDocument();
  });
});

describe("Режимы и язык", () => {
  it("режим «Разработчик» открывает журнал", async () => {
    await open();
    expect(nav().queryByRole("button", { name: "Журнал" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Разработчик" }));
    await userEvent.click(await nav().findByRole("button", { name: "Журнал" }));
    expect(await screen.findByText("Операций пока нет.")).toBeInTheDocument();
  });

  it("переключает язык RU/EN", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "EN" }));
    expect(await nav().findByRole("button", { name: "Connection" })).toBeInTheDocument();
    expect(document.documentElement.lang).toBe("en");
  });
});

describe("Подключение", () => {
  async function connect(backend?: MockBackend) {
    await open(backend);
    await goConnection();
    await userEvent.click(await screen.findByRole("button", { name: "Подключить" }));
    await screen.findByRole("button", { name: "Отключить" });
  }

  it("подключает, выполняет идентификацию и показывает evidence", async () => {
    await connect();
    await userEvent.click(screen.getByRole("button", { name: "Идентификация" }));
    await waitFor(() => expect(screen.getByTestId("outcome")).toHaveTextContent("success"));
    expect(screen.getByText(/не подтверждает работу на реальном телефоне/)).toBeInTheDocument();
  });

  it("показывает ошибку записи один раз, без автоповтора", async () => {
    const backend = new MockBackend();
    backend.failNextWrite = true;
    await connect(backend);
    await userEvent.click(screen.getByRole("button", { name: "Тестовая запись" }));
    await waitFor(() => expect(screen.getByTestId("outcome")).toHaveTextContent("failed"));
    const writes = (await backend.recentOperations(10)).filter((j) => j.operation === "write_file");
    expect(writes).toHaveLength(1);
  });

  it("показывает неподдерживаемую операцию как недоступную, а не кнопкой", async () => {
    class NoWrite extends MockBackend {
      async listDevices() {
        const devices = await super.listDevices();
        return devices.map((d) => ({
          ...d,
          capabilities: d.capabilities.map((c) =>
            c.id === "file_write" ? { ...c, status: "unsupported" as const } : c,
          ),
        }));
      }
    }
    await connect(new NoWrite());
    expect(screen.getByText("Недоступно: возможность не поддерживается")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Тестовая запись" })).toBeNull();
    expect(screen.getByRole("button", { name: "Идентификация" })).toBeInTheDocument();
  });

  it("матрица возможностей видна только разработчику", async () => {
    await connect();
    expect(screen.queryByRole("table")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Разработчик" }));
    expect(await screen.findByRole("table")).toBeInTheDocument();
  });
});

describe("Настройки", () => {
  it("содержит оговорку о независимости от Nokia", async () => {
    await open();
    await userEvent.click(nav().getByRole("button", { name: "Настройки" }));
    expect(await screen.findByText(/независимый клиент/)).toBeInTheDocument();
  });

  it("скрывает mock-устройство через настройки", async () => {
    await open();
    await userEvent.click(nav().getByRole("button", { name: "Настройки" }));
    await userEvent.click(await screen.findByLabelText(/Показывать демо-устройство/));
    await waitFor(() => expect(screen.getByText("Устройств: 0")).toBeInTheDocument());
  });
});
