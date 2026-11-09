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
    const writes = (await backend.recentOperations(10)).filter(
      (j) => j.operation === "files.write",
    );
    expect(writes).toHaveLength(1);
  });

  it("показывает неподдерживаемую операцию как недоступную, а не кнопкой", async () => {
    class NoList extends MockBackend {
      async listDevices() {
        const devices = await super.listDevices();
        return devices.map((d) => ({
          ...d,
          matrix: d.matrix && {
            ...d.matrix,
            entries: d.matrix.entries.map((e) =>
              e.operation === "files.list" ? { ...e, availability: "unsupported" as const } : e,
            ),
          },
        }));
      }
    }
    await connect(new NoList());
    expect(screen.getByText("Недоступно: возможность не поддерживается")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Список файлов" })).toBeNull();
    expect(screen.getByRole("button", { name: "Идентификация" })).toBeInTheDocument();
  });

  it("матрица соединения: запись «не проверено» до первой записи, затем «доступно»", async () => {
    await connect();
    const row = () => screen.getByText("Запись файла").closest("tr")!;
    expect(row()).toHaveTextContent("Не проверено");
    expect(screen.getByText("Чтение контактов").closest("tr")).toHaveTextContent(
      "Не поддерживается",
    );
    await userEvent.click(screen.getByRole("button", { name: "Тестовая запись" }));
    await waitFor(() => expect(row()).toHaveTextContent("Доступно"));
  });

  it("сбой записи: неизвестный исход, подсказка действия, статус «Ошибка» в матрице", async () => {
    const backend = new MockBackend();
    backend.failNextWrite = true;
    await connect(backend);
    await userEvent.click(screen.getByRole("button", { name: "Тестовая запись" }));
    expect(await screen.findByText(/Исход записи на устройстве неизвестен/)).toBeInTheDocument();
    expect(screen.getByText(/Сначала проверьте содержимое устройства/)).toBeInTheDocument();
    expect(screen.getByText("Запись файла").closest("tr")).toHaveTextContent("Ошибка");
  });

  it("операцию в работе можно отменить", async () => {
    const backend = new MockBackend();
    backend.holdMs = 5000;
    await connect(backend);
    await userEvent.click(screen.getByRole("button", { name: "Тестовая запись" }));
    await userEvent.click(await screen.findByRole("button", { name: "Отмена" }));
    await waitFor(() => expect(screen.getByTestId("outcome")).toHaveTextContent("cancelled"));
    expect(screen.getByText(/Исход записи на устройстве неизвестен/)).toBeInTheDocument();
  });

  it("зрелость, evidence и fingerprint видны только разработчику", async () => {
    await connect();
    expect(screen.queryByText("Зрелость")).toBeNull();
    expect(screen.queryByText(/Отпечаток соединения/)).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Разработчик" }));
    expect(await screen.findByText("Зрелость")).toBeInTheDocument();
    expect(screen.getByText(/Отпечаток соединения/)).toBeInTheDocument();
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

describe("Ошибки и счётчики", () => {
  it("показывает ошибку бэкенда на главной странице", async () => {
    class Broken extends MockBackend {
      async listDevices(): Promise<never> {
        throw { code: "TRANSPORT_ERROR", message: "usb недоступен", retryable: true };
      }
    }
    render(<App backend={new Broken()} />);
    expect(await screen.findByTestId("error")).toHaveTextContent("TRANSPORT_ERROR: usb недоступен");
  });

  it("не считает операции прошлых запусков операциями сессии", async () => {
    class WithHistory extends MockBackend {
      async recentOperations() {
        return [
          {
            operation_id: "old-1",
            device_ref: "dev-x",
            operation: "identify",
            state: "success" as const,
            error_code: null,
            uncertain: false,
            started_at_ms: Date.now() - 86_400_000,
            finished_at_ms: null,
          },
        ];
      }
    }
    await open(new WithHistory());
    expect(screen.getByText("0 успешно · 0 с ошибкой")).toBeInTheDocument();
  });
});

describe("Диагностика (Doctor)", () => {
  async function openDoctor() {
    await open();
    await userEvent.click(nav().getByRole("button", { name: "Диагностика" }));
    await screen.findByRole("heading", { name: "Диагностика подключения" });
  }

  it("до запуска показывает пустое состояние и обещает режим «только чтение»", async () => {
    await openDoctor();
    expect(screen.getByText("Диагностика ещё не запускалась.")).toBeInTheDocument();
    expect(screen.getByText(/Только чтение: ничего не устанавливается/)).toBeInTheDocument();
  });

  it("показывает состояние, ответ MTP и не выдаёт PC Suite/PIM за подтверждённые", async () => {
    await openDoctor();
    await userEvent.click(screen.getByRole("button", { name: "Запустить диагностику" }));
    expect(await screen.findByText("Только WPD/MTP", { selector: ".tag" })).toBeInTheDocument();
    expect(screen.getByText("MTP отвечает (чтение объектов)")).toBeInTheDocument();
    expect(screen.getByText(/0421 \(Nokia\) — модель не подтверждена/)).toBeInTheDocument();
    expect(screen.getAllByText("не подтверждено")).toHaveLength(2);
    expect(screen.getByText(/MTP\/WPD не равно PC Suite/)).toBeInTheDocument();
    expect(screen.getByText(/В отчёт не входят/)).toBeInTheDocument();
  });

  it("проверка занятости COM-порта включается явно", async () => {
    await openDoctor();
    await userEvent.click(screen.getByLabelText(/Проверять занятость COM-портов/));
    await userEvent.click(screen.getByRole("button", { name: "Запустить диагностику" }));
    expect(await screen.findByText("COM-порт занят", { selector: ".tag" })).toBeInTheDocument();
  });

  it("сохраняет отчёт и показывает путь", async () => {
    await openDoctor();
    await userEvent.click(screen.getByRole("button", { name: "Сохранить отчёт" }));
    expect(await screen.findByText(/Отчёт сохранён/)).toBeInTheDocument();
  });
});

describe("Графика телефонов", () => {
  it("до подключения — нейтральная иллюстрация без заявления модели", async () => {
    await open();
    expect(
      screen.getAllByRole("img", { name: "Условная иллюстрация телефона" }).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByRole("img", { name: /Иллюстрация телефона Nokia/ })).toBeNull();
  });

  it("после подтверждения модели показывается её иллюстрация, помеченная как не фото производителя", async () => {
    await open();
    await userEvent.click(nav().getByRole("button", { name: "Подключение" }));
    await userEvent.click(await screen.findByRole("button", { name: "Подключить" }));
    await screen.findByRole("button", { name: "Отключить" });
    const img = await screen.findAllByRole("img", { name: "Иллюстрация телефона Nokia N95" });
    expect(img[0]).toHaveAttribute("src");
    expect(screen.getAllByText(/не фото производителя/).length).toBeGreaterThan(0);
  });
});
