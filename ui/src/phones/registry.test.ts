import { findPhone, normalizeModel, PHONES } from "./registry";

describe("реестр графики телефонов", () => {
  it("нормализует обозначение модели", () => {
    expect(normalizeModel("Nokia E52-1 (MOCK)")).toBe("e52-1");
    expect(normalizeModel("  NOKIA   N95   8GB ")).toBe("n95 8gb");
  });

  it.each([
    ["Nokia E52", "nokia-e52"],
    ["E52-1", "nokia-e52"],
    ["Nokia E72", "nokia-e72"],
    ["Nokia N95-3", "nokia-n95"],
    ["Nokia N95 8GB", "nokia-n95-8gb"],
    ["N95-2", "nokia-n95-8gb"],
    ["Nokia N8-00", "nokia-n8"],
    ["Nokia E7-00", "nokia-e7"],
    ["Nokia 7610", "nokia-7610"],
    ["Nokia N97 mini", "nokia-n97-mini"],
    ["Nokia N95 (MOCK)", "nokia-n95-family"],
  ])("«%s» → %s", (model, id) => {
    expect(findPhone(model)?.id).toBe(id);
  });

  it("общее «N95» без варианта получает схематичную графику, а не конкретный корпус", () => {
    const p = findPhone("Nokia N95");
    expect(p?.provenance).toBe("schematic");
  });

  it("неизвестные и пустые модели не получают картинку", () => {
    expect(findPhone("Nokia 3310")).toBeNull();
    expect(findPhone("E5")).toBeNull(); // без префиксного угадывания
    expect(findPhone("")).toBeNull();
    expect(findPhone(null)).toBeNull();
  });

  it("у каждой записи есть изображение, источник и честный статус прав", () => {
    for (const p of PHONES) {
      expect(p.image).toBeTruthy();
      expect(p.source).toMatch(/^https:/);
      if (p.provenance === "reference-based") expect(p.rightsVerified).toBe(false);
    }
  });
});
