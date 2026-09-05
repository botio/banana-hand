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

// The chord field is a capture control, not a text box: the test must drive
// it with real keyboard events the way a user does (click to record, then
// keys). Playwright modifier names differ from the chord vocabulary.
const MODIFIER_PLAYWRIGHT = { Ctrl: "Control", Shift: "Shift", Alt: "Alt", Meta: "Meta" };

async function recordChord(page, combo) {
  const recorder = page.getByRole("button", { name: "快捷鍵組合" });
  const segments = combo.split("+");
  const modifiers = segments.slice(0, -1);
  const key = segments.at(-1);
  await recorder.click();
  for (const modifier of modifiers) await page.keyboard.down(MODIFIER_PLAYWRIGHT[modifier]);
  await page.keyboard.press(key);
  for (const modifier of modifiers) await page.keyboard.up(MODIFIER_PLAYWRIGHT[modifier]);
}

test("recording a chord and typing survive background runtime refreshes", async ({ page }) => {
  const name = page.getByRole("textbox", { name: "快捷鍵名稱" });
  const chordValue = page.locator("#chord-recorder-value");
  const hidden = page.locator('input[name="chord"]');

  await name.fill("Deployment");
  await page.clock.runFor(1_100);
  await page.keyboard.type(" confirmation");
  await expect(name).toHaveValue("Deployment confirmation");
  await expect(name).toBeFocused();

  await recordChord(page, "Ctrl+Shift+K");
  await expect(chordValue).toHaveText("Ctrl+Shift+K");
  await expect(hidden).toHaveValue("Ctrl+Shift+K");

  // The 1s polling refresh must not touch the committed chord or the draft.
  await page.clock.runFor(2_100);
  await expect(chordValue).toHaveText("Ctrl+Shift+K");
  await expect(hidden).toHaveValue("Ctrl+Shift+K");
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

test("the chord recorder rejects bare keys, commits real combos, and cancels on Escape", async ({ page }) => {
  const recorder = page.getByRole("button", { name: "快捷鍵組合" });
  const chordValue = page.locator("#chord-recorder-value");
  const hint = page.locator("#chord-recorder-hint");

  await recorder.click();
  await expect(hint).toHaveText("請按下組合鍵；Esc 取消。");

  // A bare letter has no modifier: rejected, capture stays open.
  await page.keyboard.press("KeyK");
  await expect(hint).toHaveText("主要按鍵需搭配至少一個修飾鍵（Ctrl / Shift / Alt / Meta）。");
  await expect(chordValue).toHaveText("請按下快捷鍵組合…");

  // The same capture accepts a real combo.
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("KeyK");
  await page.keyboard.up("Control");
  await page.keyboard.up("Shift");
  await expect(hint).toHaveText("");
  await expect(chordValue).toHaveText("Ctrl+Shift+K");

  // A bare F-key is a legitimate stand-alone shortcut.
  await recordChord(page, "F9");
  await expect(chordValue).toHaveText("F9");

  // Re-capturing and pressing bare Escape cancels without changing the value.
  await recorder.click();
  await expect(hint).toHaveText("請按下組合鍵；Esc 取消。");
  await page.keyboard.press("Escape");
  await expect(hint).toHaveText("");
  await expect(chordValue).toHaveText("F9");
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
            return {
              tabs: [],
              cooldown_remaining_seconds: 0,
              connected_hosts: 0,
              last_bridge_rejection: null,
              last_host_disconnect_reason: null,
            };
          case "native_host_registration":
            return {
              entries: [
                {
                  browser: "chrome",
                  manifestPath: "/tmp/nm/chrome.json",
                  registryLocation: "/tmp/nm",
                  hostPath: "/tmp/host",
                  hostExists: true,
                },
                {
                  browser: "firefox",
                  manifestPath: "/tmp/nm/firefox.json",
                  registryLocation: "/tmp/nm",
                  hostPath: "/tmp/host",
                  hostExists: true,
                },
              ],
            };
          default:
            throw new Error(`unexpected command ${command}`);
        }
      },
    };
  });
  await page.goto(url);

  const name = page.getByRole("textbox", { name: "快捷鍵名稱" });
  const chordValue = page.locator("#chord-recorder-value");
  const button = page.getByRole("button", { name: "新增快捷鍵" });

  await expect(page.locator("#connection-status")).toContainText("尚無 native host 連線");

  await name.fill("慢存確認");
  await recordChord(page, "F9");
  await page.evaluate(() => {
    window.__TAURI_TEST__.holdSave = true;
  });
  await button.click();

  // The write is still pending: submits are locked, the committed draft is untouched.
  await expect(button).toBeDisabled();
  await expect(name).toHaveValue("慢存確認");
  await expect(chordValue).toHaveText("F9");

  // The user records a newer draft while the save is still in flight.
  await name.fill("下一筆草稿");
  await recordChord(page, "Ctrl+F10");
  await expect(name).toHaveValue("下一筆草稿");
  await expect(chordValue).toHaveText("Ctrl+F10");
  await expect(button).toBeDisabled();

  // The save settles: the committed shortcut lands in the list, and the newer
  // draft must survive (a blind form.reset() would wipe it here).
  await page.evaluate(() => window.__TAURI_TEST__.releaseSave());
  await expect(button).toBeEnabled();
  await expect(page.locator("#connection-status")).toContainText("已儲存「慢存確認」");
  await expect(page.locator("#shortcut-list")).toContainText("慢存確認");
  await expect(name).toHaveValue("下一筆草稿");
  await expect(chordValue).toHaveText("Ctrl+F10");
});

test("each shortcut card has a delete button that removes it", async ({ page }) => {
  await page.addInitScript(() => {
    const state = { settings: { schemaVersion: 1, shortcuts: [] } };
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
          case "plugin:store|save":
            return null;
          case "runtime_snapshot":
            return {
              tabs: [],
              cooldown_remaining_seconds: 0,
              connected_hosts: 0,
              last_bridge_rejection: null,
              last_host_disconnect_reason: null,
            };
          case "native_host_registration":
            return {
              entries: [
                {
                  browser: "chrome",
                  manifestPath: "/tmp/nm/chrome.json",
                  registryLocation: "/tmp/nm",
                  hostPath: "/tmp/host",
                  hostExists: true,
                },
                {
                  browser: "firefox",
                  manifestPath: "/tmp/nm/firefox.json",
                  registryLocation: "/tmp/nm",
                  hostPath: "/tmp/host",
                  hostExists: true,
                },
              ],
            };
          default:
            throw new Error(`unexpected command ${command}`);
        }
      },
    };
  });
  await page.goto(url);

  // The auto-registration line reports the channels the App registered.
  await expect(page.locator("#registration-status")).toContainText("native host 已自動登錄");

  const name = page.getByRole("textbox", { name: "快捷鍵名稱" });
  await name.fill("待刪快捷鍵");
  await recordChord(page, "Ctrl+Shift+J");
  await page.getByRole("button", { name: "新增快捷鍵" }).click();
  await expect(page.locator("#shortcut-list")).toContainText("待刪快捷鍵");

  // The card's × button deletes just that shortcut.
  await page
    .getByRole("button", { name: "刪除快捷鍵：待刪快捷鍵" })
    .click();
  await expect(page.locator("#shortcut-list")).toContainText("尚無快捷鍵");
  await expect(page.locator("#connection-status")).toContainText("已刪除「待刪快捷鍵」");
});
