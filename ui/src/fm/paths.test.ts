import { formatSize, joinDevice, joinHost, parentDevice, parentHost, uniqueName } from "./paths";

describe("пути компьютера", () => {
  it("склеивает и поднимается по Windows-путям", () => {
    expect(joinHost("C:\\Users\\я", "Загрузки")).toBe("C:\\Users\\я\\Загрузки");
    expect(joinHost("C:\\", "a.txt")).toBe("C:\\a.txt");
    expect(parentHost("C:\\Users\\я")).toBe("C:\\Users");
    expect(parentHost("C:\\Users")).toBe("C:\\");
    expect(parentHost("C:\\")).toBeNull();
  });
  it("работает и с POSIX-путями", () => {
    expect(joinHost("/home/u", "x")).toBe("/home/u/x");
    expect(parentHost("/home/u")).toBe("/home");
    expect(parentHost("/home")).toBe("/");
    expect(parentHost("/")).toBeNull();
  });
});

describe("пути устройства", () => {
  it("склеивает и поднимается", () => {
    expect(joinDevice("E:/", "Images")).toBe("E:/Images");
    expect(joinDevice("/Phone memory", "DCIM")).toBe("/Phone memory/DCIM");
    expect(parentDevice("E:/Images/Фото")).toBe("E:/Images");
    expect(parentDevice("E:/Images")).toBe("E:/");
    expect(parentDevice("E:/")).toBeNull();
    expect(parentDevice("/Phone memory/DCIM")).toBe("/Phone memory");
    expect(parentDevice("/Phone memory")).toBe("/");
    expect(parentDevice("/")).toBeNull();
  });
});

describe("уникальные имена и размеры", () => {
  it("подбирает свободное имя без учёта регистра", () => {
    expect(uniqueName("a.txt", [])).toBe("a.txt");
    expect(uniqueName("a.txt", ["A.TXT"])).toBe("a (1).txt");
    expect(uniqueName("фото.jpg", ["фото.jpg", "фото (1).jpg"])).toBe("фото (2).jpg");
    expect(uniqueName(".hidden", [".hidden"])).toBe(".hidden (1)");
  });
  it("форматирует размеры", () => {
    expect(formatSize(12)).toBe("12 Б");
    expect(formatSize(2048)).toBe("2.0 КБ");
    expect(formatSize(5 * 1024 * 1024 * 1024)).toBe("5.0 ГБ");
  });
});
