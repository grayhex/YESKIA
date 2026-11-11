import assert from "node:assert/strict";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { resolve } from "node:path";
import { chromium } from "playwright";

const root = resolve(import.meta.dirname, "..");
const portProbe = createServer();
await new Promise((resolve) => portProbe.listen(0, "127.0.0.1", resolve));
const port = portProbe.address().port;
await new Promise((resolve) => portProbe.close(resolve));
const url = process.env.YESKIA_UI_URL ?? `http://127.0.0.1:${port}`;
const output =
  process.env.YESKIA_LAYOUT_OUTPUT ??
  resolve(root, "docs/screenshots/fluent/layout-report.json");
const server = process.env.YESKIA_UI_URL
  ? null
  : spawn(
      process.execPath,
      [
        resolve(root, "node_modules/vite/bin/vite.js"),
        "--host",
        "127.0.0.1",
        "--port",
        String(port),
        "--strictPort",
      ],
      { cwd: resolve(root, "ui"), stdio: "ignore" },
    );
const pages = [
  ["Главная", "Home"],
  ["Подключение", "Connection"],
  ["Агент на телефоне", "Phone agent"],
  ["Файлы", "Files"],
  ["Контакты", "Contacts"],
  ["Сообщения", "Messages"],
  ["Календарь", "Calendar"],
  ["Приложения", "Apps"],
  ["Медиа и темы", "Media and themes"],
  ["Сертификаты", "Certificates"],
  ["Резервные копии", "Backups"],
  ["Настройки", "Settings"],
  ["Помощь", "Help"],
];
const filesPage = pages.find(([ru]) => ru === "Файлы");
const agentPage = pages.find(([ru]) => ru === "Агент на телефоне");
const cases = [];
for (const [width, height] of [
  [1500, 900],
  [1366, 768],
]) {
  for (const theme of ["light", "dark"])
    for (const lang of ["ru", "en"]) {
      for (const scenario of ["connected", "empty"])
        cases.push({ width, height, theme, lang, scenario, scale: 1, pages });
      for (const scale of [1.5, 2])
        cases.push({
          width,
          height,
          theme,
          lang,
          scenario: "connected",
          scale,
          pages: [pages[0], filesPage],
        });
    }
  for (const theme of ["light", "dark"]) for (const lang of ["ru", "en"]) {
    cases.push({width,height,theme,lang,scenario:"agent",scale:1,pages:[agentPage]});
  }
  for (const scenario of ["readonly", "long"])
    cases.push({
      width,
      height,
      theme: "light",
      lang: "ru",
      scenario,
      scale: 1,
      pages: [filesPage],
    });
  cases.push({
    width,
    height,
    theme: "light",
    lang: "ru",
    scenario: "welcome",
    scale: 1,
    pages: [],
  });
}
let browser;
const checks = [];
const errors = [];
async function checkLayout(page, c, screen) {
  // Readiness belongs to every screen, including the welcome branch below.
  // Failed decodes remain visible to the brokenImages assertion.
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all(
      [...document.images].map((image) =>
        image.decode().catch(() => undefined),
      ),
    );
  });
  const layout = await page.evaluate(() => {
    const main = document.querySelector("main");
    const nav = document.querySelector("nav");
    const scrolling = [...main.querySelectorAll("*")]
      .filter((el) => {
        const css = getComputedStyle(el);
        return (
          /auto|scroll/.test(css.overflowY) &&
          el.clientHeight > 0 &&
          el.scrollHeight > el.clientHeight + 2
        );
      })
      .map((el) => ({
        container: el.className,
        verticalOverflow: el.scrollHeight - el.clientHeight,
      }));
    return {
      bodyX: Math.max(0, document.documentElement.scrollWidth - innerWidth),
      bodyY: Math.max(0, document.documentElement.scrollHeight - innerHeight),
      mainX: main.scrollWidth - main.clientWidth,
      mainY: main.scrollHeight - main.clientHeight,
      navY: nav.scrollHeight - nav.clientHeight,
      scrolling,
      brokenImages: [...document.images]
        .filter((img) => !img.complete || !img.naturalWidth)
        .map((img) => img.src),
      missingIcons: [
        ...document.querySelectorAll("svg:not(.hero-orbits)"),
      ].filter((svg) => !svg.querySelector("path[d],rect,circle")).length,
      iconCount: document.querySelectorAll("svg").length,
    };
  });
  checks.push({ ...c, pages: undefined, screen, ...layout });
  assert.equal(layout.bodyX, 0, `${screen}: body overflows horizontally`);
  assert.equal(layout.bodyY, 0, `${screen}: shell overflows vertically`);
  assert.equal(layout.mainX, 0, `${screen}: main overflows horizontally`);
  assert.deepEqual(layout.brokenImages, [], `${screen}: missing images`);
  assert.equal(layout.missingIcons, 0, `${screen}: empty SVG`);
  assert.equal(
    await page.locator("nav .nav-item > svg").count(),
    await page.locator("nav .nav-item").count(),
    "each navigation item must have a line icon",
  );
  if (screen === "Home" && c.scenario === "connected") {
    for (const tone of ["blue", "green", "purple", "amber"])
      assert.equal(
        await page.locator(`.quick-actions .accent-${tone}`).count(),
        1,
        `missing ${tone} action accent`,
      );
    assert.match(
      await page.locator(".device-hero .phone img").getAttribute("src"),
      /nokia-e72/,
    );
    assert.equal(
      await page.locator(".brand img, .sidebar-brand img").count(),
      0,
      "branding must remain text",
    );
  }
  if (c.scale === 1 && !["Preparation", "TLS Revival", "Contact editor"].includes(screen)) {
    assert.ok(
      layout.mainY <= 2,
      `${c.width} ${c.lang} ${c.scenario} ${screen}: unnecessary main scroll (${layout.mainY}px)`,
    );
    assert.ok(
      layout.navY <= 2,
      `${screen}: unnecessary sidebar scroll (${layout.navY}px)`,
    );
  }
  if (c.scenario === "long") {
    assert.equal(
      layout.scrolling.filter((el) => el.container.includes("pane-body"))
        .length,
      2,
      "long file lists must scroll inside both panes",
    );
  }
}
try {
  for (let n = 0; n < 100; n++) {
    try {
      if ((await fetch(url)).ok) break;
    } catch {
      /* server is starting */
    }
    if (n === 99) throw new Error("Vite did not start");
    await new Promise((r) => setTimeout(r, 100));
  }
  browser = await chromium.launch({
    executablePath:
      process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH ??
      (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined),
    args: ["--no-sandbox"],
  });
  async function worker() {
    while (cases.length) {
      const c = cases.shift();
      const context = await browser.newContext({
        viewport: {
          width: Math.round(c.width / c.scale),
          height: Math.round(c.height / c.scale),
        },
        deviceScaleFactor: c.scale,
        colorScheme: c.theme,
        reducedMotion: "reduce",
      });
      const page = await context.newPage();
      if (c.scenario === "welcome") {
        // Regress the welcome image-loading race even on a warm, fast runner.
        await page.route("**/*.webp", async (route) => {
          await new Promise((resolve) => setTimeout(resolve, 1000));
          await route.continue();
        });
      }
      page.on("pageerror", (error) => errors.push(error.message));
      try {
        await page.goto(
          `${url}/tests/layout/index.html?scenario=${c.scenario}&lang=${c.lang}`,
          { waitUntil: "domcontentloaded" },
        );
        await page.locator(".header-controls button:enabled").first().waitFor();
        if (c.scenario === "welcome") {
          await page.getByTestId("wel-step").waitFor();
          await checkLayout(page, c, "welcome");
        }
        for (const names of c.pages) {
          await page
            .getByRole("navigation")
            .getByRole("button", {
              name: names[c.lang === "ru" ? 0 : 1],
              exact: true,
            })
            .click();
          if (names[0] === "Файлы") {
            await page
              .getByRole("grid", {
                name: c.lang === "ru" ? "Компьютер" : "Computer",
              })
              .getByText("old.txt", { exact: true })
              .waitFor();
            if (c.scenario !== "empty")
              await page
                .getByRole("grid", {
                  name: c.lang === "ru" ? "Телефон" : "Phone",
                })
                .getByText("readme.txt", { exact: true })
                .waitFor();
          }
          if (names[0] === "Агент на телефоне" && c.scenario === "agent") {
            await page.getByRole("button", {name:"Device Lab",exact:true}).click();
          }
          await checkLayout(page, c, names[1]);
          if (c.scenario === "readonly") {
            assert.equal(
              await page
                .getByRole("button", { name: /Загрузить на телефон/ })
                .count(),
              0,
            );
            await page
              .getByText(
                "Запись на это устройство недоступна: соединение только для чтения.",
              )
              .waitFor();
          }
        }
        if (c.scenario === "connected") {
          const nav=page.getByRole("navigation");
          await nav.getByRole("button",{name:c.lang==="ru"?"Главная":"Home",exact:true}).click();
          await page.getByRole("button",{name:c.lang==="ru"?"Подготовка E72":"Prepare E72",exact:true}).click();
          await page.getByRole("heading",{name:c.lang==="ru"?"Подготовка E72":"Prepare E72",exact:true}).waitFor();
          await checkLayout(page,c,"Preparation");
          await nav.getByRole("button",{name:c.lang==="ru"?"Сертификаты":"Certificates",exact:true}).click();
          await page.getByRole("button",{name:"TLS / HTTPS Revival",exact:true}).click();
          await page.getByText(/d20d6d41c796d53f/).waitFor();
          await checkLayout(page,c,"TLS Revival");
          await nav.getByRole("button",{name:c.lang==="ru"?"Контакты":"Contacts",exact:true}).click();
          await page.getByRole("button",{name:c.lang==="ru"?"Новый контакт":"New contact",exact:true}).click();
          await checkLayout(page,c,"Contact editor");
        }
        if (c.scale === 1 && c.scenario === "connected") {
          await page
            .getByRole("button", {
              name: c.lang === "ru" ? "Разработчик" : "Developer",
              exact: true,
            })
            .click();
          await page
            .getByRole("navigation")
            .getByRole("button", {
              name: c.lang === "ru" ? "Журнал" : "Journal",
              exact: true,
            })
            .click();
          await checkLayout(page, c, "Journal");
        }
      } catch (error) {
        errors.push(
          `${JSON.stringify({ ...c, pages: undefined })}: ${error.message}`,
        );
      } finally {
        await context.close();
      }
    }
  }
  await Promise.all([worker(), worker(), worker()]);
  if (errors.length) process.exitCode = 1;
} catch (error) {
  errors.push(error.message);
  process.exitCode = 1;
} finally {
  await browser?.close();
  server?.kill();
  mkdirSync(resolve(output, ".."), { recursive: true });
  // One record per line keeps the generated matrix readable in a PR diff.
  const lines = [
    "{",
    '  "checks": [',
    ...checks.map(
      (check, index) =>
        `    ${JSON.stringify(check)}${index < checks.length - 1 ? "," : ""}`,
    ),
    "  ],",
    `  "errors": ${JSON.stringify(errors)}`,
    "}",
    "",
  ];
  writeFileSync(output, lines.join(String.fromCharCode(10)));
  if (process.env.GITHUB_ACTIONS === "true") {
    for (const error of errors) {
      const message = error
        .replaceAll("%", "%25")
        .replaceAll("\r", "%0D")
        .replaceAll("\n", "%0A");
      console.error(`::error title=Layout regression::${message}`);
    }
  }
  console.log(JSON.stringify({ checks: checks.length, errors }, null, 2));
}
