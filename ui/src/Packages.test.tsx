import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockBackend } from "./mockBackend";

const nav = () => within(screen.getByRole("navigation"));
const DOWNLOADS = "C:\\Users\\user\\Downloads";

function backendWithFiles() {
  const b = new MockBackend();
  b.hostFiles[`${DOWNLOADS}\\Игра.sis`] = 1500;
  b.hostFiles[`${DOWNLOADS}\\App.jar`] = 2000;
  b.hostFiles[`${DOWNLOADS}\\broken.sis`] = 99;
  b.packageSamples[`${DOWNLOADS}\\Игра.sis`] = {
    size: 1500,
    title: "Игра",
    vendor: "Автор",
    uid: "0xe0000809",
    version: "1.0.0",
    platform_uids: ["0x101f7961"],
  };
  b.packageSamples[`${DOWNLOADS}\\broken.sis`] = {
    size: 99,
    status: "corrupt",
    error: "контрольные суммы или хеши файлов не сходятся",
  };
  return b;
}

async function openPackages(b = backendWithFiles()) {
  render(<App backend={b} />);
  await screen.findByText("Nokia N95 (MOCK)");
  await userEvent.click(nav().getByRole("button", { name: "Приложения" }));
  await screen.findByRole("heading", { name: "Приложения и пакеты" });
  return b;
}

async function importAll(b: MockBackend) {
  b.nextPackageFiles = [
    `${DOWNLOADS}\\Игра.sis`,
    `${DOWNLOADS}\\App.jar`,
    `${DOWNLOADS}\\broken.sis`,
  ];
  await userEvent.click(screen.getByRole("button", { name: "Добавить файлы…" }));
  await screen.findByText("Добавлено файлов: 3.");
}

async function connect() {
  await userEvent.click(nav().getByRole("button", { name: "Подключение" }));
  await userEvent.click(await screen.findByRole("button", { name: "Подключить" }));
  await screen.findByRole("button", { name: "Отключить" });
  await userEvent.click(nav().getByRole("button", { name: "Приложения" }));
  await screen.findByRole("heading", { name: "Приложения и пакеты" });
}

describe("Приложения и пакеты", () => {
  it("показывает границы возможностей и пустую библиотеку", async () => {
    await openPackages();
    expect(screen.getByRole("note")).toHaveTextContent(/ничего не устанавливает/);
    expect(screen.getByText("В библиотеке нет подходящих пакетов.")).toBeInTheDocument();
  });

  it("импорт из файлов, поиск и фильтр по формату", async () => {
    const b = await openPackages();
    await importAll(b);
    expect(await screen.findByText("Игра")).toBeInTheDocument();
    expect(screen.getByText("Повреждён")).toBeInTheDocument();
    await userEvent.type(screen.getByRole("textbox", { name: "Поиск по библиотеке" }), "игра");
    await waitFor(() => expect(screen.queryByText("broken")).not.toBeInTheDocument());
    expect(screen.getByText("Игра")).toBeInTheDocument();
    await userEvent.clear(screen.getByRole("textbox", { name: "Поиск по библиотеке" }));
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Формат" }), "jar");
    await waitFor(() => expect(screen.queryByText("Игра")).not.toBeInTheDocument());
    expect(screen.getByText("App")).toBeInTheDocument();
  });

  it("фильтр по модели показывает заявленную платформу, а не совместимость", async () => {
    const b = await openPackages();
    await importAll(b);
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Модель телефона" }),
      "nokia-e52",
    );
    expect(await screen.findByText("заявлена")).toBeInTheDocument();
    expect(screen.getByText(/Это не проверка установки/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("checkbox", { name: /Только с заявленной/ }));
    await waitFor(() => expect(screen.queryByText("broken")).not.toBeInTheDocument());
    expect(screen.getByText("Игра")).toBeInTheDocument();
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Модель телефона" }),
      "nokia-n97-mini",
    );
    await waitFor(() => expect(screen.queryByText("Игра")).not.toBeInTheDocument());
  });

  it("анализ: «неизвестно», подпись не равна доверию, установка не проверялась", async () => {
    const b = await openPackages();
    await importAll(b);
    await userEvent.click(await screen.findByRole("button", { name: "Анализ: Игра.sis" }));
    expect(await screen.findByTestId("pk-compat")).toHaveTextContent("Совместимость: неизвестна");
    expect(screen.getByText(/не проверялись/)).toBeInTheDocument();
    expect(screen.getByTestId("pk-sig")).toHaveTextContent("Пакет не подписан");
    expect(screen.getByTestId("pk-deps")).toHaveTextContent("Зависимости не объявлены");
    expect(screen.getByTestId("pk-issues")).toHaveTextContent("unsigned-package");
  });

  it("повреждённый пакет — «несовместим», и его нельзя передать", async () => {
    const b = await openPackages();
    await importAll(b);
    await connect();
    await userEvent.click(await screen.findByRole("button", { name: "Анализ: broken.sis" }));
    expect(await screen.findByTestId("pk-compat")).toHaveTextContent("Несовместим");
    expect(screen.getByTestId("pk-issues")).toHaveTextContent("corrupt-package");
    expect(screen.getByRole("button", { name: "Передать на телефон… broken.sis" })).toBeDisabled();
  });

  it("сохраняет отчёт и разбирает сообщение установщика", async () => {
    const b = await openPackages();
    await importAll(b);
    await userEvent.click(await screen.findByRole("button", { name: "Анализ: Игра.sis" }));
    await userEvent.click(await screen.findByRole("button", { name: "Сохранить отчёт" }));
    expect(await screen.findByText(/Отчёт сохранён/)).toBeInTheDocument();
    await userEvent.type(
      screen.getByRole("textbox", { name: "Текст ошибки с телефона" }),
      "Certificate expired",
    );
    await userEvent.click(screen.getByRole("button", { name: "Разобрать" }));
    expect(await screen.findByTestId("pk-diagnosis")).toHaveTextContent("expired-certificate");
    expect(screen.getByText(/не доказательство/)).toBeInTheDocument();
  });

  it("наборы: создать, добавить пакет, фильтровать, удалить", async () => {
    const b = await openPackages();
    await importAll(b);
    await userEvent.type(screen.getByRole("textbox", { name: "Название набора" }), "Мои игры");
    await userEvent.click(screen.getByRole("button", { name: "Создать набор" }));
    expect(await screen.findByText(/Мои игры · Пакетов: 0/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "В набор… Игра.sis" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "В набор…" }));
    expect(await screen.findByText(/Мои игры · Пакетов: 1/)).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Наборы приложений" }), "1");
    await waitFor(() => expect(screen.queryByText("App")).not.toBeInTheDocument());
    expect(screen.getByText("Игра")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Удалить набор «Мои игры»/ }));
    await waitFor(() => expect(screen.queryByText(/Мои игры ·/)).not.toBeInTheDocument());
  });

  it("передача на телефон недоступна без устройства и называет причину", async () => {
    const b = await openPackages();
    await importAll(b);
    expect(screen.getByRole("button", { name: "Передать на телефон… Игра.sis" })).toBeDisabled();
    expect(screen.getByText(/Подключите устройство/)).toBeInTheDocument();
  });

  it("передача: подтверждение «это не установка», запись, повтор не перезаписывает", async () => {
    const b = await openPackages();
    await importAll(b);
    await connect();
    await userEvent.click(
      await screen.findByRole("button", { name: "Передать на телефон… Игра.sis" }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Это не установка");
    expect(dialog).toHaveTextContent("не перезаписывается");
    await userEvent.click(within(dialog).getByRole("button", { name: "Записать файл" }));
    await waitFor(() => expect(screen.getByTestId("pk-transfer")).toHaveTextContent("успешно"));
    expect(screen.getByText(/Установка не выполнялась/)).toBeInTheDocument();
    expect(b.deviceFiles["E:/Игра.sis"]).toBe(1500);
    // повтор
    await userEvent.click(screen.getByRole("button", { name: "Передать на телефон… Игра.sis" }));
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Записать файл" }),
    );
    await waitFor(() => expect(screen.getByTestId("pk-transfer")).toHaveTextContent("ошибка"));
    expect(screen.getByText(/ALREADY_EXISTS/)).toBeInTheDocument();
  });

  it("удаление из библиотеки не трогает телефон", async () => {
    const b = await openPackages();
    await importAll(b);
    await userEvent.click(
      await screen.findByRole("button", { name: "Удалить из библиотеки App.jar" }),
    );
    await waitFor(() => expect(screen.queryByText("App")).not.toBeInTheDocument());
    expect(Object.keys(b.deviceFiles)).toHaveLength(3);
  });
});
