import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockBackend } from "./mockBackend";
import { App } from "./App";

async function connected(backend = new MockBackend()) {
  render(<App backend={backend} />);
  await userEvent.click(await screen.findByRole("button", { name: "Подключить" }));
  await screen.findByRole("button", { name: "Отключить" });
  return backend;
}

describe("App (UI-заглушка)", () => {
  it("явно помечает UI-заглушку и mock-устройство", async () => {
    render(<App backend={new MockBackend()} />);
    expect(await screen.findByText(/UI-заглушка/)).toBeInTheDocument();
    expect(await screen.findByText("MOCK")).toBeInTheDocument();
    expect(screen.getByText(/s\/n \*\*\*09/)).toBeInTheDocument();
  });

  it("выполняет идентификацию и показывает evidence", async () => {
    await connected();
    await userEvent.click(screen.getByRole("button", { name: "Идентификация" }));
    await waitFor(() => expect(screen.getByTestId("outcome")).toHaveTextContent("success"));
    expect(screen.getByText(/не подтверждает работу на реальном телефоне/)).toBeInTheDocument();
  });

  it("показывает структурированную ошибку записи без автоповтора", async () => {
    const backend = new MockBackend();
    backend.failNextWrite = true;
    await connected(backend);
    await userEvent.click(screen.getByRole("button", { name: "Тестовая запись" }));
    await waitFor(() => expect(screen.getByTestId("outcome")).toHaveTextContent("failed"));
    expect(screen.getAllByText(/OPERATION_FAILED/).length).toBeGreaterThan(0);
    expect(
      (await backend.recentOperations(10)).filter((j) => j.operation === "write_file"),
    ).toHaveLength(1);
  });

  it("скрывает mock-устройство через настройки", async () => {
    render(<App backend={new MockBackend()} />);
    await screen.findByText("MOCK");
    await userEvent.click(screen.getByLabelText(/Показывать mock-устройство/));
    expect(await screen.findByText("Устройства не найдены.")).toBeInTheDocument();
  });
});
