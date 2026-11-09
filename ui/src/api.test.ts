import { toErrorInfo } from "./api";

describe("toErrorInfo", () => {
  it("сохраняет структурированную ошибку ядра", () => {
    const e = { code: "NOT_CONNECTED", message: "x", retryable: false };
    expect(toErrorInfo(e)).toBe(e);
  });
  it("приводит неизвестные ошибки к INTERNAL", () => {
    expect(toErrorInfo("boom").code).toBe("INTERNAL");
  });
});
