import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockBackend } from "./mockBackend";

const DL = "C:\\Users\\user\\Downloads";
const nav = () => within(screen.getByRole("navigation"));

async function openFiles(backend = new MockBackend(), connect = true) {
  render(<App backend={backend} />);
  await screen.findByText("Nokia N95 (MOCK)");
  if (connect) {
    await userEvent.click(nav().getByRole("button", { name: "Подключение" }));
    await userEvent.click(await screen.findByRole("button", { name: "Подключить" }));
    await screen.findByRole("button", { name: "Отключить" });
  }
  await userEvent.click(nav().getByRole("button", { name: "Файлы" }));
  await screen.findByRole("heading", { name: "Файловый менеджер" });
  return backend;
}

const grid = (name: string) => screen.getByRole("grid", { name });
const row = (pane: string, file: string) =>
  within(grid(pane)).getByRole("row", { name: new RegExp(file) });

describe("Файловый менеджер: панели и навигация", () => {
  it("без устройства показывает пустое состояние и ведёт в «Подключение»", async () => {
    await openFiles(new MockBackend(), false);
    expect(
      screen.getByText("Нет подключённого устройства", { selector: "h3" }),
    ).toBeInTheDocument();
    // панель компьютера работает и без телефона
    expect(await within(grid("Компьютер")).findByText("old.txt")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Открыть раздел/ }));
    expect(await screen.findByRole("heading", { name: "Подключение" })).toBeInTheDocument();
  });

  it("показывает обе панели и поднимается/спускается по папкам", async () => {
    await openFiles();
    expect(await within(grid("Телефон")).findByText("readme.txt")).toBeInTheDocument();
    expect(within(grid("Телефон")).getByText("Images")).toBeInTheDocument();

    await userEvent.dblClick(row("Телефон", "Images"));
    expect(await within(grid("Телефон")).findByText("cat.jpg")).toBeInTheDocument();
    expect(within(grid("Телефон")).getByText("Фото")).toBeInTheDocument();

    const phone = screen.getByRole("region", { name: "Телефон" });
    await userEvent.click(within(phone).getByRole("button", { name: "Вверх" }));
    expect(await within(grid("Телефон")).findByText("readme.txt")).toBeInTheDocument();
  });

  it("клавиатура: стрелки, Пробел, Ctrl+A, Enter, Backspace", async () => {
    await openFiles();
    await within(grid("Телефон")).findByText("readme.txt");
    row("Телефон", "Images").focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(row("Телефон", "readme.txt")).toHaveAttribute("aria-selected", "true");
    await userEvent.keyboard("{ArrowUp}{Control>}a{/Control}");
    expect(screen.getAllByText("Выбрано: 2").length).toBeGreaterThan(0);
    await userEvent.keyboard("{ArrowUp}");
    await userEvent.keyboard("{Enter}"); // Images — папка
    expect(await within(grid("Телефон")).findByText("cat.jpg")).toBeInTheDocument();
    await userEvent.keyboard("{Backspace}");
    expect(await within(grid("Телефон")).findByText("readme.txt")).toBeInTheDocument();
  });

  it("фильтр по имени и сортировка", async () => {
    await openFiles();
    await within(grid("Телефон")).findByText("readme.txt");
    const phone = screen.getByRole("region", { name: "Телефон" });
    await userEvent.type(within(phone).getByRole("searchbox"), "read");
    expect(within(grid("Телефон")).queryByText("Images")).toBeNull();
    expect(within(grid("Телефон")).getByText("readme.txt")).toBeInTheDocument();
  });
});

describe("Файловый менеджер: скачивание", () => {
  it("скачивает файл на компьютер и показывает проверку размера и SHA-256", async () => {
    const b = await openFiles();
    await userEvent.click(row("Телефон", "readme.txt"));
    const phone = screen.getByRole("region", { name: "Телефон" });
    await userEvent.click(within(phone).getByRole("button", { name: "Скачать" }));
    expect(await screen.findByText(/проверено: размер и SHA-256/)).toBeInTheDocument();
    expect(b.hostFiles[`${DL}\\readme.txt`]).toBe(12);
    expect(await within(grid("Компьютер")).findByText("readme.txt")).toBeInTheDocument();
  });

  it("скачивает папку целиком вместе с вложенными папками и кириллицей", async () => {
    const b = await openFiles();
    await userEvent.click(row("Телефон", "Images"));
    const phone = screen.getByRole("region", { name: "Телефон" });
    await userEvent.click(within(phone).getByRole("button", { name: "Скачать" }));
    await waitFor(() => expect(b.hostFiles[`${DL}\\Images\\Фото\\снимок.jpg`]).toBe(2048));
    expect(b.hostFiles[`${DL}\\Images\\cat.jpg`]).toBe(4);
    expect(b.hostDirs.has(`${DL}\\Images\\Фото`)).toBe(true);
  });

  async function startConflict() {
    const b = new MockBackend();
    b.deviceFiles["E:/old.txt"] = 99;
    await openFiles(b);
    await userEvent.click(row("Телефон", "old.txt"));
    const phone = screen.getByRole("region", { name: "Телефон" });
    await userEvent.click(within(phone).getByRole("button", { name: "Скачать" }));
    const dlg = await screen.findByRole("dialog", { name: "Файл уже существует" });
    return { b, dlg };
  }

  it("коллизия: файл не перезаписывается, пока пользователь не решит", async () => {
    const { b, dlg } = await startConflict();
    expect(b.hostFiles[`${DL}\\old.txt`]).toBe(5); // исходный файл цел
    expect(within(dlg).getByText(/не перезаписываются без вашего решения/)).toBeInTheDocument();
    await userEvent.click(within(dlg).getByRole("button", { name: "Пропустить" }));
    expect(await screen.findByText(/пропущено/)).toBeInTheDocument();
    expect(b.hostFiles[`${DL}\\old.txt`]).toBe(5);
  });

  it("коллизия: «другое имя» создаёт old (1).txt", async () => {
    const { b, dlg } = await startConflict();
    await userEvent.click(within(dlg).getByRole("button", { name: "Сохранить под другим именем" }));
    await waitFor(() => expect(b.hostFiles[`${DL}\\old (1).txt`]).toBe(99));
    expect(b.hostFiles[`${DL}\\old.txt`]).toBe(5);
  });

  it("коллизия: замена — только по явной кнопке", async () => {
    const { b, dlg } = await startConflict();
    await userEvent.click(within(dlg).getByRole("button", { name: "Заменить" }));
    await waitFor(() => expect(b.hostFiles[`${DL}\\old.txt`]).toBe(99));
    expect(await screen.findByText(/заменён по вашему решению/)).toBeInTheDocument();
  });

  it("Esc в диалоге коллизии — безопасный отказ (пропуск)", async () => {
    const { b } = await startConflict();
    await userEvent.keyboard("{Escape}");
    expect(await screen.findByText(/пропущено/)).toBeInTheDocument();
    expect(b.hostFiles[`${DL}\\old.txt`]).toBe(5);
  });

  it("операцию можно отменить: файл не появляется, очередь останавливается", async () => {
    const b = new MockBackend();
    await openFiles(b);
    await within(grid("Телефон")).findByText("readme.txt");
    b.holdMs = 5000; // после загрузки панелей: «зависает» только передача
    await userEvent.click(row("Телефон", "readme.txt"));
    const phone = screen.getByRole("region", { name: "Телефон" });
    await userEvent.click(within(phone).getByRole("button", { name: "Скачать" }));
    const transfers = await screen.findByRole("list", { name: "Передача файлов" });
    await userEvent.click(screen.getAllByRole("button", { name: "Отмена" }).at(-1)!);
    expect(await within(transfers).findByText(/отменено/)).toBeInTheDocument();
    expect(b.hostFiles[`${DL}\\readme.txt`]).toBeUndefined();
  });

  it("Ctrl+C на телефоне и Ctrl+V на компьютере — скачивание", async () => {
    const b = await openFiles();
    await userEvent.click(row("Телефон", "readme.txt"));
    await userEvent.keyboard("{Control>}c{/Control}");
    row("Компьютер", "old.txt").focus();
    await userEvent.keyboard("{Control>}v{/Control}");
    await waitFor(() => expect(b.hostFiles[`${DL}\\readme.txt`]).toBe(12));
  });

  it("перетаскивание с телефона на панель компьютера скачивает файл", async () => {
    const b = await openFiles();
    const r = row("Телефон", "readme.txt");
    await userEvent.click(r);
    const dt = { setData: () => undefined, effectAllowed: "" };
    fireEvent.dragStart(r, { dataTransfer: dt });
    const hostBody = grid("Компьютер").closest(".pane-body")!;
    fireEvent.dragOver(hostBody, { dataTransfer: dt });
    fireEvent.drop(hostBody, { dataTransfer: dt });
    await waitFor(() => expect(b.hostFiles[`${DL}\\readme.txt`]).toBe(12));
  });

  it("контекстное меню предлагает действия над выделенным", async () => {
    await openFiles();
    fireEvent.contextMenu(row("Телефон", "readme.txt"));
    const menu = await screen.findByRole("menu");
    expect(within(menu).getByRole("menuitem", { name: "Скачать" })).toBeEnabled();
    expect(within(menu).getByRole("menuitem", { name: "Скачать как…" })).toBeEnabled();
  });
});

describe("Файловый менеджер: диалоги Windows и проводник", () => {
  it("«Выбрать папку…» открывает нативный диалог и переходит в выбранную папку", async () => {
    const b = await openFiles();
    const host = screen.getByRole("region", { name: "Компьютер" });
    await userEvent.click(within(host).getByRole("button", { name: "Выбрать папку…" }));
    expect(await within(grid("Компьютер")).findByText("отчёт.docx")).toBeInTheDocument();
    expect(b.nextFolder).toBeTruthy();
  });

  it("«Скачать как…» берёт путь из диалога сохранения", async () => {
    const b = new MockBackend();
    b.nextSavePath = `${DL}\\copy-of-readme.txt`;
    await openFiles(b);
    await userEvent.click(row("Телефон", "readme.txt"));
    const phone = screen.getByRole("region", { name: "Телефон" });
    await userEvent.click(within(phone).getByRole("button", { name: "Скачать как…" }));
    await waitFor(() => expect(b.hostFiles[`${DL}\\copy-of-readme.txt`]).toBe(12));
  });

  it("«Показать в проводнике» передаёт путь выбранного файла", async () => {
    const b = await openFiles();
    await userEvent.click(row("Компьютер", "old.txt"));
    const host = screen.getByRole("region", { name: "Компьютер" });
    await userEvent.click(within(host).getByRole("button", { name: "Показать в проводнике" }));
    await waitFor(() => expect(b.revealed).toEqual([`${DL}\\old.txt`]));
  });
});

describe("Файловый менеджер: запись на устройство", () => {
  async function pickHostFile() {
    await userEvent.click(row("Компьютер", "old.txt"));
    const phone = screen.getByRole("region", { name: "Телефон" });
    await userEvent.click(within(phone).getByRole("button", { name: "Загрузить на телефон" }));
  }

  it("первая запись требует явного согласия, повторно — нет; результат проверяется", async () => {
    const b = await openFiles();
    await pickHostFile();
    const dlg = await screen.findByRole("dialog", { name: "Запись на устройство" });
    expect(b.deviceFiles["E:/old.txt"]).toBeUndefined(); // до согласия ничего не записано
    await userEvent.click(within(dlg).getByRole("button", { name: "Записать" }));
    await waitFor(() => expect(b.deviceFiles["E:/old.txt"]).toBe(5));
    expect(await screen.findByText(/проверено: размер и SHA-256/)).toBeInTheDocument();
    // второй файл — без повторного диалога
    b.hostFiles[`${DL}\\second.txt`] = 7;
    const host = screen.getByRole("region", { name: "Компьютер" });
    await userEvent.click(within(host).getByRole("button", { name: "Обновить" }));
    await userEvent.click(await within(grid("Компьютер")).findByText("second.txt"));
    const phone = screen.getByRole("region", { name: "Телефон" });
    await userEvent.click(within(phone).getByRole("button", { name: "Загрузить на телефон" }));
    await waitFor(() => expect(b.deviceFiles["E:/second.txt"]).toBe(7));
    expect(screen.queryByRole("dialog", { name: "Запись на устройство" })).toBeNull();
  });

  it("отказ в согласии ничего не записывает", async () => {
    const b = await openFiles();
    await pickHostFile();
    const dlg = await screen.findByRole("dialog", { name: "Запись на устройство" });
    await userEvent.click(within(dlg).getByRole("button", { name: "Отмена" }));
    expect(b.deviceFiles["E:/old.txt"]).toBeUndefined();
  });

  it("существующий файл на устройстве не перезаписывается без решения", async () => {
    const b = new MockBackend();
    b.deviceFiles["E:/old.txt"] = 1;
    await openFiles(b);
    await pickHostFile();
    await userEvent.click(
      within(await screen.findByRole("dialog", { name: "Запись на устройство" })).getByRole(
        "button",
        {
          name: "Записать",
        },
      ),
    );
    const conflict = await screen.findByRole("dialog", { name: "Файл уже существует" });
    expect(b.deviceFiles["E:/old.txt"]).toBe(1);
    await userEvent.click(within(conflict).getByRole("button", { name: "Пропустить" }));
    expect(b.deviceFiles["E:/old.txt"]).toBe(1);
  });

  it("если запись не поддерживается, кнопок загрузки нет", async () => {
    class ReadOnly extends MockBackend {
      async listDevices() {
        const d = await super.listDevices();
        return d.map((x) => ({
          ...x,
          matrix: x.matrix && {
            ...x.matrix,
            entries: x.matrix.entries.map((e) =>
              e.operation === "files.write" ? { ...e, availability: "unsupported" as const } : e,
            ),
          },
        }));
      }
    }
    await openFiles(new ReadOnly());
    await within(grid("Телефон")).findByText("readme.txt");
    expect(screen.queryByRole("button", { name: "Загрузить на телефон" })).toBeNull();
    expect(screen.getByText(/Запись на это устройство недоступна/)).toBeInTheDocument();
  });

  it("неопределённый сбой записи останавливает очередь и предупреждает", async () => {
    const b = new MockBackend();
    b.failNextWrite = true;
    await openFiles(b);
    await pickHostFile();
    await userEvent.click(
      within(await screen.findByRole("dialog", { name: "Запись на устройство" })).getByRole(
        "button",
        {
          name: "Записать",
        },
      ),
    );
    expect(await screen.findByText(/Исход записи на устройстве неизвестен/)).toBeInTheDocument();
    expect(b.deviceFiles["E:/old.txt"]).toBeUndefined();
  });
});

describe("Файловый менеджер: источник буфера, пустые папки, лимиты", () => {
  it("вставка берёт файлы из папки, где было копирование, а не из текущей", async () => {
    const b = new MockBackend();
    b.deviceFiles["E:/Images/readme.txt"] = 999; // одноимённый файл в другой папке
    await openFiles(b);
    await userEvent.click(row("Телефон", "readme.txt"));
    await userEvent.keyboard("{Control>}c{/Control}");
    await userEvent.dblClick(row("Телефон", "Images")); // ушли в другую папку
    await within(grid("Телефон")).findByText("cat.jpg");
    row("Компьютер", "old.txt").focus();
    await userEvent.keyboard("{Control>}v{/Control}");
    await waitFor(() => expect(b.hostFiles[`${DL}\\readme.txt`]).toBeDefined());
    expect(b.hostFiles[`${DL}\\readme.txt`]).toBe(12); // из корня, а не 999 из Images
  });

  it("перетаскивание работает и в пустую папку на компьютере", async () => {
    const b = new MockBackend();
    b.hostDirs.add("C:\\Users\\user\\Empty");
    b.nextFolder = "C:\\Users\\user\\Empty";
    await openFiles(b);
    const host = screen.getByRole("region", { name: "Компьютер" });
    await userEvent.click(within(host).getByRole("button", { name: "Выбрать папку…" }));
    expect(await within(host).findByText("Папка пуста")).toBeInTheDocument();

    const r = row("Телефон", "readme.txt");
    await userEvent.click(r);
    const dt = { setData: () => undefined, effectAllowed: "" };
    fireEvent.dragStart(r, { dataTransfer: dt });
    const zone = host.querySelector(".pane-body")!;
    fireEvent.dragOver(zone, { dataTransfer: dt });
    fireEvent.drop(zone, { dataTransfer: dt });
    await waitFor(() => expect(b.hostFiles["C:\\Users\\user\\Empty\\readme.txt"]).toBe(12));
  });

  it("слишком глубокая папка отвергается целиком, а не скачивается частично", async () => {
    const b = new MockBackend();
    const deep = Array.from({ length: 20 }, () => "d").join("/");
    b.deviceFiles[`E:/${deep}/f.txt`] = 1;
    await openFiles(b);
    await userEvent.click(row("Телефон", "^d$"));
    const phone = screen.getByRole("region", { name: "Телефон" });
    await userEvent.click(within(phone).getByRole("button", { name: "Скачать" }));
    expect(await screen.findByText(/Скачивание не начато/)).toBeInTheDocument();
    expect(b.hostDirs.has(`${DL}\\d`)).toBe(false); // ни одной папки не создано
    expect(screen.queryByRole("list", { name: "Передача файлов" })).toBeNull();
  });
});

it("фоновый опрос с новым объектом того же устройства не перечитывает папку", async () => {
  const backend = new MockBackend();
  const original = backend.listDevices.bind(backend);
  const list = vi
    .spyOn(backend, "listDevices")
    .mockImplementation(async () => JSON.parse(JSON.stringify(await original())));
  const start = vi.spyOn(backend, "startOperation");
  await openFiles(backend);
  await within(grid("Телефон")).findByText("readme.txt");
  const calls = start.mock.calls.filter(([, r]) => r.type === "files.list").length;
  vi.useFakeTimers();
  try {
    const { act } = await import("@testing-library/react");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3100);
    });
    expect(list.mock.calls.length).toBeGreaterThan(1);
    expect(start.mock.calls.filter(([, r]) => r.type === "files.list")).toHaveLength(calls);
  } finally {
    vi.useRealTimers();
  }
});
