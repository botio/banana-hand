import { expect, test } from "@playwright/test";
import { createServer } from "vite";

let server;
let url;

test.use({
  launchOptions: {
    executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH,
  },
});

test.beforeAll(async () => {
  server = await createServer({
    logLevel: "error",
    server: { host: "127.0.0.1", port: 1421, strictPort: false },
  });
  await server.listen();
  url = server.resolvedUrls.local[0];
});

test.afterAll(async () => {
  await server?.close();
});

test.beforeEach(async ({ page }) => {
  await page.clock.install();
  await page.goto(url);
});

test("typing and text selection survive background runtime refreshes", async ({ page }) => {
  const name = page.getByRole("textbox", { name: "快捷鍵名稱" });
  const chord = page.getByRole("textbox", { name: "快捷鍵組合" });
  await name.fill("Deployment");
  await page.clock.runFor(1_100);
  await page.keyboard.type(" confirmation");
  await expect(name).toHaveValue("Deployment confirmation");
  await expect(name).toBeFocused();

  await chord.fill("Ctrl+Shift+K");
  await chord.evaluate((input) => input.setSelectionRange(5, 10));
  await page.clock.runFor(2_100);
  await page.keyboard.type("Alt");
  await expect(chord).toHaveValue("Ctrl+Alt+K");
  await expect(chord).toBeFocused();
  await expect(name).toHaveValue("Deployment confirmation");
});

test("Chinese IME composition survives a runtime refresh", async ({ page }) => {
  const name = page.getByRole("textbox", { name: "快捷鍵名稱" });
  await name.focus();
  const cdp = await page.context().newCDPSession(page);
  try {
    await cdp.send("Input.imeSetComposition", {
      text: "部署",
      selectionStart: 2,
      selectionEnd: 2,
    });
    await page.clock.runFor(1_100);
    await cdp.send("Input.insertText", { text: "部署確認" });
    await expect(name).toHaveValue("部署確認");
    await expect(name).toBeFocused();
  } finally {
    await cdp.detach();
  }
});

test("a draft typed during a pending save is not reset when the save settles", async ({ page }) => {
  await page.addInitScript(() => {
    const state = { settings: { schemaVersion: 1, shortcuts: [] } };
    window.__TAURI_TEST__ = {
      holdSave: false,
      releaseSave: null,
    };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: "main" },
        currentWebview: { label: "main" },
      },
      async invoke(command, payload) {
        switch (command) {
          case "plugin:store|load":
            return "test-store-rid";
          case "plugin:store|get":
            return [state.settings, true];
          case "plugin:store|set":
            state.settings = payload.value;
            return null;
          case "plugin:store|save": {
            if (window.__TAURI_TEST__.holdSave) {
              window.__TAURI_TEST__.holdSave = false;
              await new Promise((resolve) => {
                window.__TAURI_TEST__.releaseSave = resolve;
              });
            }
            return null;
          }
          case "runtime_snapshot":
            return { tabs: [], cooldown_remaining_seconds: 0 };
          default:
            throw new Error(`unexpected command ${command}`);
        }
      },
    };
  });
  await page.goto(url);

  const name = page.getByRole("textbox", { name: "快捷鍵名稱" });
  const chord = page.getByRole("textbox", { name: "快捷鍵組合" });
  const button = page.getByRole("button", { name: "新增快捷鍵" });

  await expect(page.locator("#connection-status")).toContainText("Browser Tab");

  await name.fill("慢存確認");
  await chord.fill("F9");
  await page.evaluate(() => {
    window.__TAURI_TEST__.holdSave = true;
  });
  await button.click();

  // The write is still pending: submits are locked, the committed draft is untouched.
  await expect(button).toBeDisabled();
  await expect(name).toHaveValue("慢存確認");
  await expect(chord).toHaveValue("F9");

  // The user types a newer draft while the save is still in flight.
  await name.fill("下一筆草稿");
  await chord.fill("Ctrl+F10");
  await expect(name).toHaveValue("下一筆草稿");
  await expect(chord).toHaveValue("Ctrl+F10");
  await expect(button).toBeDisabled();

  // The save settles: the committed shortcut lands in the list, and the newer
  // draft must survive (a blind form.reset() would wipe it here).
  await page.evaluate(() => window.__TAURI_TEST__.releaseSave());
  await expect(button).toBeEnabled();
  await expect(page.locator("#connection-status")).toContainText("已儲存「慢存確認」");
  await expect(page.locator("#shortcut-list")).toContainText("慢存確認");
  await expect(name).toHaveValue("下一筆草稿");
  await expect(chord).toHaveValue("Ctrl+F10");
});
