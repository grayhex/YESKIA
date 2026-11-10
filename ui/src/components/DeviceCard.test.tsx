import { render, screen } from "@testing-library/react";
import { DeviceCard } from "./DeviceCard";
import { MockBackend } from "../mockBackend";
import { I18nProvider } from "../i18n";

async function card(name: string, identified: string | null = null) {
  const [device] = await new MockBackend().listDevices();
  render(
    <I18nProvider lang="ru">
      <DeviceCard device={{ ...device, display_name: name, identified_model: identified }} />
    </I18nProvider>,
  );
}

it.each([
  ["Nokia E52", "nokia-e52.webp"],
  ["Nokia E72", "nokia-e72.webp"],
  ["Nokia N95", "nokia-n95.webp"],
  ["Nokia N95 8GB", "nokia-n95-8gb.webp"],
  ["Nokia N97 mini", "nokia-n97-mini.webp"],
  ["Nokia N8-00", "nokia-n8.webp"],
  ["Nokia E7-00", "nokia-e7.webp"],
  ["Nokia 7610", "nokia-7610.webp"],
])("карточка %s использует графику s60modern до подключения", async (name, file) => {
  await card(name);
  expect(screen.getByRole("img", { name: `Иллюстрация телефона ${name}` })).toHaveAttribute(
    "src",
    expect.stringContaining(file),
  );
});

it("подтверждённая модель важнее имени устройства", async () => {
  await card("Nokia N95", "Nokia N95 8GB");
  expect(screen.getByRole("img", { name: "Иллюстрация телефона Nokia N95 8GB" })).toHaveAttribute(
    "src",
    expect.stringContaining("nokia-n95-8gb.webp"),
  );
});

it("неизвестная модель использует нейтральную графику", async () => {
  await card("Nokia USB device");
  expect(screen.getByRole("img", { name: "Условная иллюстрация телефона" })).toBeInTheDocument();
});
