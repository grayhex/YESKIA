import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockBackend } from "./mockBackend";
import { SCREENS } from "./mediaPresets";
import { PHONES } from "./phones/registry";
import type { MediaInfo, ThemeReport } from "./types";

const nav = () => within(screen.getByRole("navigation"));
const SONG = "C:\\Users\\user\\Downloads\\song.mp3";
const PIC = "C:\\Users\\user\\Downloads\\photo.png";
const clip: MediaInfo = {
  kind: "audio",
  duration: 95.5,
  container: "mp3",
  video: null,
  audio: { codec: "mp3", width: null, height: null, sample_rate: 44100, channels: 2 },
  size: 3_000_000,
};
const pic: MediaInfo = {
  kind: "image",
  duration: 0,
  container: "png_pipe",
  video: { codec: "png", width: 1000, height: 500, sample_rate: null, channels: null },
  audio: null,
  size: 50_000,
};

const themeReport = (over: Partial<ThemeReport> = {}): ThemeReport => ({
  schema: "yeskia.theme/1",
  name: "e52clearlight",
  bitmaps: [{ width: 240, height: 320, bpp: 16, compression: 3 }],
  icons: 46,
  skn_pid1: 0xe0e52c01,
  package: {
    source: "pkg",
    name: "E52 Clear Light",
    uid: 0xe0e52c01,
    vendor: "e52modern",
    platform_deps: [0x101f7961],
    package_deps: [],
    targets: [],
  },
  signed: null,
  uid_class: "test",
  checks: [
    { id: "mbm.structure", status: "pass", detail: "71 растров" },
    { id: "uid.range", status: "warn", detail: "тестовый диапазон" },
    { id: "firmware.acceptance", status: "unknown", detail: "не устанавливается" },
  ],
  structure: "consistent",
  firmware_acceptance: "unknown",
  installation: "not-tested",
  physical_device: "not-tested",
  palette: [{ hex: "#F8F8F8", share: 0.5, contrast_on_white: 1.07, contrast_on_black: 19.6 }],
  limitations: ["Предпросмотр — растры из .mbm"],
  ...over,
});

function setup() {
  const b = new MockBackend();
  b.mediaFiles[SONG] = clip;
  b.mediaFiles[PIC] = pic;
  b.hostFiles[SONG] = 3_000_000;
  b.hostFiles[PIC] = 50_000;
  return b;
}

async function open(b = setup()) {
  render(<App backend={b} />);
  await screen.findByText("Nokia N95 (MOCK)");
  await userEvent.click(nav().getByRole("button", { name: "Медиа и темы" }));
  await screen.findByRole("heading", { name: "Медиа и темы" });
  return b;
}

async function pick(b: MockBackend, path: string) {
  b.nextMediaFile = path;
  await userEvent.click(screen.getByRole("button", { name: "Выбрать файл…" }));
}

describe("Медиа и темы", () => {
  it("показывает версию и лицензию FFmpeg и называет его внешним инструментом", async () => {
    await open();
    expect(await screen.findByTestId("st-tool")).toHaveTextContent(
      "FFmpeg 6.1-ui-mock, лицензия сборки: lgpl",
    );
    expect(screen.getByText(/внешний инструмент пользователя/)).toBeInTheDocument();
    expect(screen.getByRole("note")).toHaveTextContent(/без загрузки на серверы/);
  });

  it("без FFmpeg сообщает об этом, рингтон не создаётся, темы работают", async () => {
    const b = setup();
    b.ffmpegAvailable = false;
    b.themeSamples["C:\\themes\\t"] = themeReport();
    await open(b);
    expect(await screen.findByTestId("st-tool-missing")).toHaveTextContent(/TOOL_MISSING/);
    await pick(b, SONG);
    expect(await screen.findByTestId("error")).toHaveTextContent(/FFmpeg/);
    b.nextFolder = "C:\\themes\\t";
    await userEvent.click(screen.getByRole("button", { name: "Папка темы…" }));
    expect(await screen.findByTestId("st-theme-structure")).toHaveTextContent("согласованы");
  });

  it("рингтон: проверка файла, волна, заготовка, создание и состояния результата", async () => {
    const b = await open();
    await pick(b, SONG);
    expect(await screen.findByTestId("st-info")).toHaveTextContent("Аудио · 95.5 с · mp3");
    expect(await screen.findByRole("img", { name: "Волна" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Рингтон: 30 с/ }));
    expect(screen.getByRole("textbox", { name: "Конец, с" })).toHaveValue("30");
    await userEvent.click(screen.getByRole("button", { name: /Сигнал: 5 с/ }));
    expect(screen.getByRole("textbox", { name: "Конец, с" })).toHaveValue("5");
    expect(screen.getByRole("combobox", { name: "Битрейт, кбит/с" })).toHaveValue("96");
    b.nextSavePath = "C:\\Users\\user\\Downloads\\song-ringtone.mp3";
    await userEvent.click(screen.getByRole("button", { name: "Создать рингтон…" }));
    expect(await screen.findByTestId("st-outcome")).toHaveTextContent("успешно");
    expect(b.hostFiles["C:\\Users\\user\\Downloads\\song-ringtone.mp3"]).toBe(4096);
    const stages = screen.getByTestId("st-stages");
    expect(stages).toHaveTextContent("Создан на компьютере: да, файл создан и перечитан");
    expect(stages).toHaveTextContent("Передан на телефон: нет");
    expect(stages).toHaveTextContent("Установлен / назначен: приложение этого не определяет");
    expect(stages).toHaveTextContent("Проверен на телефоне: не подтверждено");
    await userEvent.click(screen.getByRole("button", { name: "Прослушать результат" }));
    expect(await screen.findByTestId("st-audio")).toBeInTheDocument();
  });

  it("повторное сохранение в тот же файл — ошибка без перезаписи", async () => {
    const b = await open();
    await pick(b, SONG);
    await screen.findByRole("img", { name: "Волна" });
    b.nextSavePath = "C:\\Users\\user\\Downloads\\old.txt";
    await userEvent.click(screen.getByRole("button", { name: "Создать рингтон…" }));
    expect(await screen.findByTestId("st-outcome")).toHaveTextContent("ошибка");
    expect(screen.getByText(/ALREADY_EXISTS/)).toBeInTheDocument();
    expect(b.hostFiles["C:\\Users\\user\\Downloads\\old.txt"]).toBe(5);
  });

  it("создание можно остановить", async () => {
    const b = await open();
    b.holdMs = 5000;
    await pick(b, SONG);
    await screen.findByRole("img", { name: "Волна" });
    b.nextSavePath = "C:\\Users\\user\\Downloads\\cancel.mp3";
    await userEvent.click(screen.getByRole("button", { name: "Создать рингтон…" }));
    await userEvent.click(await screen.findByRole("button", { name: "Остановить" }));
    expect(await screen.findByTestId("st-outcome")).toHaveTextContent("отменено");
    expect("C:\\Users\\user\\Downloads\\cancel.mp3" in b.hostFiles).toBe(false);
  });

  it("передача результата — только при подключённом устройстве и после подтверждения", async () => {
    const b = await open();
    await pick(b, SONG);
    await screen.findByRole("img", { name: "Волна" });
    b.nextSavePath = "C:\\Users\\user\\Downloads\\song-ringtone.mp3";
    await userEvent.click(screen.getByRole("button", { name: "Создать рингтон…" }));
    await screen.findByTestId("st-outcome");
    expect(screen.getByRole("button", { name: "Передать на телефон…" })).toBeDisabled();
    expect(screen.getByText(/Подключите устройство/)).toBeInTheDocument();
    // подключаем телефон
    await userEvent.click(nav().getByRole("button", { name: "Подключение" }));
    await userEvent.click(await screen.findByRole("button", { name: "Подключить" }));
    await screen.findByRole("button", { name: "Отключить" });
    await userEvent.click(nav().getByRole("button", { name: "Медиа и темы" }));
    await pick(b, SONG);
    await screen.findByRole("img", { name: "Волна" });
    b.nextSavePath = "C:\\Users\\user\\Downloads\\two.mp3";
    await userEvent.click(screen.getByRole("button", { name: "Создать рингтон…" }));
    await screen.findByTestId("st-outcome");
    await userEvent.click(screen.getByRole("button", { name: "Передать на телефон…" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("не перезаписывается");
    await userEvent.click(within(dialog).getByRole("button", { name: "Записать файл" }));
    await waitFor(() =>
      expect(screen.getByTestId("st-stages")).toHaveTextContent("Передан на телефон: да"),
    );
    expect(b.deviceFiles["E:/two.mp3"]).toBe(4096);
    expect(screen.getByTestId("st-stages")).toHaveTextContent(
      "Проверен на телефоне: не подтверждено",
    );
  });

  it("пресеты картинок зависят от экрана выбранной модели и не обещают совместимость", async () => {
    const b = await open();
    await pick(b, PIC);
    expect(await screen.findByTestId("st-info")).toHaveTextContent("Картинка · 1000×500");
    expect(screen.queryByRole("heading", { name: "Рингтон" })).not.toBeInTheDocument();
    expect(screen.getByText(/Пресеты теоретические/)).toBeInTheDocument();
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Пресет" }),
      "wallpaper-jpeg",
    );
    expect(screen.getByRole("button", { name: "Подготовить файл…" })).toBeDisabled();
    expect(screen.getByText("Для картинок выберите модель телефона.")).toBeInTheDocument();
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Модель телефона" }),
      "nokia-e52",
    );
    expect(screen.getByTestId("st-screen")).toHaveTextContent("240×320");
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Модель телефона" }),
      "nokia-e72",
    );
    expect(screen.getByTestId("st-screen")).toHaveTextContent("320×240");
    b.nextSavePath = "C:\\Users\\user\\Downloads\\wall.jpg";
    await userEvent.click(screen.getByRole("button", { name: "Подготовить файл…" }));
    expect(await screen.findByTestId("st-outcome")).toHaveTextContent("успешно");
    expect(screen.getByTestId("st-stages")).toHaveTextContent("Передан на телефон: нет");
  });

  it("тема: проверки, палитра и растры; принятие прошивкой неизвестно", async () => {
    const b = setup();
    b.themeSamples["C:\\themes\\e52"] = themeReport();
    b.themeSamples["C:\\themes\\bad"] = themeReport({
      structure: "broken",
      checks: [{ id: "mbm.structure", status: "fail", detail: "обрезан" }],
    });
    await open(b);
    b.nextFolder = "C:\\themes\\e52";
    await userEvent.click(screen.getByRole("button", { name: "Папка темы…" }));
    expect(await screen.findByTestId("st-theme-structure")).toHaveTextContent("Файлы согласованы");
    expect(
      screen.getByText("Принятие прошивкой: неизвестно · установка: не проверялась."),
    ).toBeInTheDocument();
    expect(screen.getByTestId("st-theme-checks")).toHaveTextContent("firmware.acceptance");
    expect(screen.getByText(/#F8F8F8/)).toBeInTheDocument();
    expect(await screen.findAllByRole("img", { name: /240×320/ })).toHaveLength(3);
    expect(
      screen.getByText("Это растры из файла .mbm, а не вид системы на телефоне."),
    ).toBeInTheDocument();
    b.nextFolder = "C:\\themes\\bad";
    await userEvent.click(screen.getByRole("button", { name: "Папка темы…" }));
    await waitFor(() =>
      expect(screen.getByTestId("st-theme-structure")).toHaveTextContent(
        "найдены ошибки структуры",
      ),
    );
    // непонятная папка — ошибка, а не пустой «успех»
    b.nextFolder = "C:\\themes\\none";
    await userEvent.click(screen.getByRole("button", { name: "Папка темы…" }));
    expect(await screen.findByTestId("error")).toBeInTheDocument();
  });
});

describe("Пресеты", () => {
  it("для каждой модели реестра есть документированный размер экрана", () => {
    for (const p of PHONES) expect(SCREENS[p.id], p.id).toBeDefined();
    expect(SCREENS["nokia-e52"]).toEqual([240, 320]);
    expect(SCREENS["nokia-e72"]).toEqual([320, 240]);
  });
});
