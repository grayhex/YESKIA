import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockBackend } from "./mockBackend";

const DOCS = "C:\\Users\\user\\Documents";
const nav = () => within(screen.getByRole("navigation"));

async function openBackups(backend = new MockBackend()) {
  render(<App backend={backend} />);
  await screen.findByText("Nokia N95 (MOCK)");
  await userEvent.click(nav().getByRole("button", { name: "Резервные копии" }));
  await screen.findByRole("heading", { name: "Резервные копии файлов" });
  return backend;
}

describe("Резервные копии файлов", () => {
  it("показывает границы возможностей и пустой список", async () => {
    await openBackups();
    expect(screen.getByRole("note")).toHaveTextContent(/файл/i);
    expect(screen.getByText("Копий пока нет.")).toBeInTheDocument();
  });

  it("создаёт копию из локальной папки и показывает её в списке", async () => {
    const backend = await openBackups();
    backend.nextFolder = DOCS;
    const pick = screen.getAllByRole("button", { name: "Выбрать папку…" });
    await userEvent.click(pick[pick.length - 1]!);
    await screen.findByText(DOCS);
    await userEvent.click(screen.getByRole("button", { name: "Создать копию" }));
    await waitFor(() => expect(screen.getByTestId("bk-outcome")).toHaveTextContent("успешно"));
    expect(backend.backups).toHaveLength(1);
    expect(screen.getByText(/Скопировано файлов: 1/)).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Проверить" })).toBeInTheDocument();
  });

  it("проверка целой копии успешна, повреждённая — нет", async () => {
    const backend = await openBackups();
    const good = backend.newBackupForTest("good", { "a.txt": 3 });
    const bad = backend.newBackupForTest("bad", { "b.txt": 4 });
    backend.corruptBackups.add(bad);
    await userEvent.click(nav().getByRole("button", { name: "Файлы" }));
    await userEvent.click(nav().getByRole("button", { name: "Резервные копии" }));
    const rows = await screen.findAllByRole("row");
    const rowOf = (n: string) => rows.find((r) => r.textContent?.includes(n))!;
    await userEvent.click(within(rowOf(good)).getByRole("button", { name: "Проверить" }));
    await waitFor(() => expect(screen.getByTestId("bk-verify")).toHaveTextContent(/Копия цела/));
    await userEvent.click(within(rowOf(bad)).getByRole("button", { name: "Проверить" }));
    await waitFor(() =>
      expect(screen.getByTestId("bk-verify")).toHaveTextContent(/не прошла проверку/),
    );
    expect(screen.getByText("Повреждены")).toBeInTheDocument();
  });

  it("восстановление: план без записи, затем подтверждение и только новые файлы", async () => {
    const backend = await openBackups();
    const name = backend.newBackupForTest("r", { "new.txt": 7, "отчёт.docx": 1024, "old.txt": 9 });
    backend.hostFiles[`${DOCS}\\отчёт.docx`] = 1024;
    backend.hostFiles[`${DOCS}\\old.txt`] = 5; // иной размер → конфликт
    backend.nextFolder = DOCS;
    await userEvent.click(nav().getByRole("button", { name: "Файлы" }));
    await userEvent.click(nav().getByRole("button", { name: "Резервные копии" }));
    const row = (await screen.findAllByRole("row")).find((r) => r.textContent?.includes(name))!;
    await userEvent.click(within(row).getByRole("button", { name: "Восстановить…" }));
    const panel = screen.getByRole("heading", { name: `Восстановление: ${name}` }).parentElement!;
    await userEvent.click(within(panel).getByRole("button", { name: "Выбрать папку…" }));
    await userEvent.click(within(panel).getByRole("button", { name: "Показать план" }));
    await waitFor(() =>
      expect(screen.getByTestId("bk-plan-counts")).toHaveTextContent(
        "Новых: 1 · совпадают: 1 · конфликтов: 1 · заблокировано: 0",
      ),
    );
    expect(`${DOCS}\\new.txt` in backend.hostFiles).toBe(false);

    await userEvent.click(screen.getByRole("button", { name: "Восстановить новые файлы (1)" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("не перезаписываются");
    await userEvent.click(within(dialog).getByRole("button", { name: "Записать" }));
    await waitFor(() =>
      expect(screen.getByTestId("bk-applied")).toHaveTextContent("Записано новых файлов: 1"),
    );
    expect(backend.hostFiles[`${DOCS}\\new.txt`]).toBe(7);
    expect(backend.hostFiles[`${DOCS}\\old.txt`]).toBe(5); // конфликтующий не тронут
  });

  it("восстановление повреждённой копии отклоняется", async () => {
    const backend = await openBackups();
    const name = backend.newBackupForTest("c", { "x.txt": 1 });
    backend.corruptBackups.add(name);
    backend.nextFolder = "C:\\Users\\user\\Downloads";
    await userEvent.click(nav().getByRole("button", { name: "Файлы" }));
    await userEvent.click(nav().getByRole("button", { name: "Резервные копии" }));
    const row = (await screen.findAllByRole("row")).find((r) => r.textContent?.includes(name))!;
    await userEvent.click(within(row).getByRole("button", { name: "Восстановить…" }));
    const panel = screen.getByRole("heading", { name: `Восстановление: ${name}` }).parentElement!;
    await userEvent.click(within(panel).getByRole("button", { name: "Выбрать папку…" }));
    await userEvent.click(within(panel).getByRole("button", { name: "Показать план" }));
    await userEvent.click(await screen.findByRole("button", { name: /Восстановить новые файлы/ }));
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Записать" }),
    );
    await waitFor(() => expect(screen.getByTestId("bk-outcome")).toHaveTextContent("ошибка"));
    expect(screen.getByText(/BACKUP_INVALID/)).toBeInTheDocument();
    expect(backend.hostFiles["C:\\Users\\user\\Downloads\\x.txt"]).toBeUndefined();
  });

  it("создание можно остановить", async () => {
    const backend = await openBackups();
    backend.holdMs = 5000;
    backend.nextFolder = DOCS;
    const pick = screen.getAllByRole("button", { name: "Выбрать папку…" });
    await userEvent.click(pick[pick.length - 1]!);
    await screen.findByText(DOCS);
    await userEvent.click(screen.getByRole("button", { name: "Создать копию" }));
    await userEvent.click(await screen.findByRole("button", { name: "Остановить" }));
    await waitFor(() => expect(screen.getByTestId("bk-outcome")).toHaveTextContent("отменено"));
  });

  it("источник — устройство недоступен без подключения", async () => {
    await openBackups();
    expect(screen.getByRole("radio", { name: "Папка подключённого устройства" })).toBeDisabled();
    expect(screen.getByText(/не подключено/)).toBeInTheDocument();
  });

  it("с подключённым устройством копирует папку телефона", async () => {
    const backend = new MockBackend();
    render(<App backend={backend} />);
    await screen.findByText("Nokia N95 (MOCK)");
    await userEvent.click(nav().getByRole("button", { name: "Подключение" }));
    await userEvent.click(await screen.findByRole("button", { name: "Подключить" }));
    await screen.findByRole("button", { name: "Отключить" });
    await userEvent.click(nav().getByRole("button", { name: "Резервные копии" }));
    await userEvent.click(
      await screen.findByRole("radio", { name: "Папка подключённого устройства" }),
    );
    const path = screen.getByRole("textbox", { name: "Путь на устройстве" });
    await userEvent.clear(path);
    await userEvent.type(path, "E:/Images");
    await userEvent.click(screen.getByRole("button", { name: "Создать копию" }));
    await waitFor(() => expect(screen.getByTestId("bk-outcome")).toHaveTextContent("успешно"));
    expect(backend.backups).toHaveLength(1);
  });
});
