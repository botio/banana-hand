import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { chromium, expect } from "@playwright/test";

// Run only in a disposable Windows CI account: the installed app registers
// its host and persists the shortcut through the normal user-facing workflow.
assert.equal(process.platform, "win32");
const [executable, diagnostics] = process.argv.slice(2);
assert.ok(executable && diagnostics, "Usage: node verify-windows-browser.mjs <desktop.exe> <diagnostics-directory>");
await mkdir(diagnostics, { recursive: true });
const temporary = await mkdtemp(path.join(tmpdir(), "banana-browser-smoke-"));
const logs = [];
let desktop;
let browserContext;
let webview;
let app;
const targetPages = [];
const reloadMode = process.env.BANANA_SMOKE_CHORD !== "F8";
const loads = [0, 0];
const server = createServer((request, response) => {
  const index = request.url === "/first" ? 0 : request.url === "/second" ? 1 : -1;
  if (index === -1) { response.writeHead(404).end(); return; }
  loads[index] += 1;
  logs.push(`Document request target ${index + 1} at ${Date.now()}, count ${loads[index]}`);
  const name = index === 0 ? "first" : "second";
  response.setHeader("Cache-Control", "no-store");
  response.setHeader("Content-Type", "text/html; charset=utf-8");
  response.end(`<!doctype html><title>Banana smoke ${name}</title><h1>${name}</h1><output id="keys">0</output><script>
    window.receivedKeys = [];
    window.inputTrace = [];
    const trace = (type, event) => inputTrace.push({
      type, at: Date.now(), focused: document.hasFocus(),
      visibility: document.visibilityState,
      key: event?.key, code: event?.code, trusted: event?.isTrusted
    });
    addEventListener('focus', event => trace('focus', event));
    addEventListener('blur', event => trace('blur', event));
    document.addEventListener('visibilitychange', event => trace('visibilitychange', event));
    addEventListener('keyup', event => trace('keyup', event));
    addEventListener('keydown', event => {
      if (!${reloadMode}) event.preventDefault();
      trace('keydown', event);
      receivedKeys.push({key:event.key, code:event.code, trusted:event.isTrusted});
      document.querySelector('#keys').textContent = receivedKeys.length;
    });
  </script>`);
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
const watchdog = setTimeout(() => {
  logs.push("FAIL: browser smoke exceeded 240 seconds");
  void finish().finally(() => process.exit(1));
}, 240_000);

async function finish() {
  clearTimeout(watchdog);
  for (const [index, page] of targetPages.entries()) {
    const evidence = await page.evaluate(() => ({
      keys: receivedKeys, trace: inputTrace, focused: document.hasFocus(),
      visibility: document.visibilityState,
    })).catch(error => ({ error: error.message }));
    await writeFile(path.join(diagnostics, `target-${index + 1}.json`), JSON.stringify(evidence, null, 2));
  }
  if (app) {
    try {
      execFileSync("pwsh", ["-NoProfile", "-Command", `
        Add-Type -AssemblyName System.Windows.Forms
        Add-Type -AssemblyName System.Drawing
        $bounds = [Windows.Forms.SystemInformation]::VirtualScreen
        $bitmap = [Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
          $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bitmap.Size)
          $bitmap.Save($env:BANANA_SMOKE_SCREENSHOT, [Drawing.Imaging.ImageFormat]::Png)
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
      `], { env: { ...process.env, BANANA_SMOKE_SCREENSHOT: path.join(diagnostics, "foreground.png") }, timeout: 5000 });
    } catch (error) { logs.push(`Desktop screenshot unavailable: ${error.message}`); }
    await app.screenshot({ path: path.join(diagnostics, "dispatch.png"), timeout: 3000 }).catch(() => {});
    logs.push(`Desktop result: ${await app.locator("#result").textContent({ timeout: 1000 }).catch(() => "unavailable")}`);
  }
  await writeFile(path.join(diagnostics, "browser-smoke.log"), logs.join("\n"));
  await browserContext?.close().catch(() => {});
  // Disconnect CDP, then stop only our retained desktop process tree.
  await webview?.close().catch(() => {});
  if (desktop && desktop.exitCode === null) {
    try { execFileSync("taskkill", ["/PID", String(desktop.pid), "/T", "/F"], { stdio: "ignore" }); } catch {}
  }
  server.closeAllConnections();
  server.close();
}

try {
  desktop = spawn(path.resolve(executable), [], {
    cwd: path.dirname(path.resolve(executable)),
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
      WEBVIEW2_USER_DATA_FOLDER: path.join(temporary, "webview"),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  desktop.stdout.on("data", data => logs.push(`desktop stdout: ${data}`));
  desktop.stderr.on("data", data => logs.push(`desktop stderr: ${data}`));
  const deadline = Date.now() + 30_000;
  let connectionError;
  while (Date.now() < deadline) {
    assert.equal(desktop.exitCode, null, "desktop exited before WebView2 became ready");
    try {
      webview = await chromium.connectOverCDP("http://127.0.0.1:9222", { timeout: 1000 });
      break;
    } catch (error) {
      connectionError = error;
      await delay(200);
    }
  }
  if (!webview) {
    const processEvidence = execFileSync("pwsh", ["-NoProfile", "-Command", `
      $all = @(Get-CimInstance Win32_Process)
      $ids = @(${desktop.pid})
      do {
        $children = @($all | Where-Object { $_.ParentProcessId -in $ids -and $_.ProcessId -notin $ids })
        $ids += @($children | ForEach-Object { $_.ProcessId })
      } while ($children.Count -gt 0)
      $app = Get-Process -Id ${desktop.pid}
      @{
        Window = @($app | Select-Object Id, MainWindowHandle, MainWindowTitle, Responding)
        Processes = @($all | Where-Object { $_.ProcessId -in $ids } | Select-Object ProcessId, ParentProcessId, Name, CommandLine)
        Listeners = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { $_.OwningProcess -in $ids } | Select-Object LocalAddress, LocalPort, OwningProcess)
      } | ConvertTo-Json -Depth 5
    `], { encoding: "utf8", timeout: 15_000 });
    logs.push(processEvidence);
  }
  assert.ok(webview, `WebView2 CDP did not become available: ${connectionError}`);
  await expect.poll(() => webview.contexts().flatMap(context => context.pages()).length).toBeGreaterThan(0);
  app = webview.contexts().flatMap(context => context.pages())[0];
  await app.locator("#dispatch").waitFor();

  const extension = path.resolve("extensions/chromium");
  const channel = process.env.BANANA_SMOKE_CHANNEL ?? "chrome";
  browserContext = await chromium.launchPersistentContext(path.join(temporary, "chromium"), {
    channel,
    headless: false,
    args: channel === "chrome" ? [] : [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`],
  });
  logs.push(`Browser channel ${channel}: ${browserContext.browser()?.version() ?? "unknown"}`);
  if (channel === "chrome") {
    const extensionsPage = await browserContext.newPage();
    await extensionsPage.goto("chrome://extensions");
    await extensionsPage.locator("extensions-manager").evaluate(manager => {
      const toolbar = manager.shadowRoot.querySelector("extensions-toolbar").shadowRoot;
      const devMode = toolbar.querySelector("#devMode");
      if (!devMode.checked) devMode.click();
    });
    const [chooser] = await Promise.all([
      extensionsPage.waitForEvent("filechooser", { timeout: 10_000 }),
      extensionsPage.locator("extensions-manager").evaluate(manager => {
        manager.shadowRoot.querySelector("extensions-toolbar").shadowRoot.querySelector("#loadUnpacked").click();
      }),
    ]);
    await chooser.setFiles(extension);
    await extensionsPage.close();
  }
  const worker = browserContext.serviceWorkers()[0] ?? await browserContext.waitForEvent("serviceworker");
  worker.on("console", message => logs.push(`extension: ${message.text()}`));
  const first = await browserContext.newPage();
  await first.goto(`${origin}/first`);
  const firstTab = await worker.evaluate(async () => {
    const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
    return { id: tab.id, windowId: tab.windowId };
  });
  const secondPage = browserContext.waitForEvent("page", { timeout: 10_000 });
  await worker.evaluate(async ({ windowId, url }) => {
    await chrome.tabs.create({ windowId, url, active: false });
  }, { windowId: firstTab.windowId, url: `${origin}/second` });
  const second = await secondPage;
  targetPages.push(first, second);
  const windows = await worker.evaluate(async () => (await chrome.tabs.query({})).map(tab => ({
    id: tab.id, windowId: tab.windowId, title: tab.title, url: tab.url, active: tab.active,
  })));
  logs.push(`Chrome tabs: ${JSON.stringify(windows)}`);
  const smokeWindows = new Set(windows.filter(tab => tab.url?.includes("/first") || tab.url?.includes("/second")).map(tab => tab.windowId));
  assert.equal(smokeWindows.size, 1, "targets must share one Chrome window");
  await first.goto(`${origin}/first`);
  await second.goto(`${origin}/second`);
  await expect(app.locator("#first-target option").filter({ hasText: "Banana smoke first" })).toHaveCount(1, { timeout: 30_000 });
  const firstValue = await app.locator("#first-target option").filter({ hasText: "Banana smoke first" }).getAttribute("value");
  const secondValue = await app.locator("#second-target option").filter({ hasText: "Banana smoke second" }).getAttribute("value");
  assert.ok(firstValue && secondValue);
  logs.push("PASS: real extension hello and both tab snapshots reached desktop");

  await app.getByRole("textbox", { name: "快捷鍵名稱" }).fill(reloadMode ? "Windows smoke Ctrl+R" : "Windows smoke F8");
  await app.getByRole("button", { name: "快捷鍵組合" }).click();
  await app.keyboard.press(reloadMode ? "Control+r" : "F8");
  await app.getByRole("button", { name: "新增快捷鍵" }).click();
  await app.locator("#first-target").selectOption(firstValue);
  await app.locator("#second-target").selectOption(secondValue);
  const failures = [];
  const pageKeys = () => Promise.all(targetPages.map(page => page.evaluate(() => receivedKeys)));
  let previousDispatchAt;
  for (let round = 1; round <= 2; round += 1) {
    if (previousDispatchAt !== undefined) {
      // Use the real cooldown. Do not restart the app, clear its state, or
      // foreground either target between rounds.
      await delay(Math.max(0, previousDispatchAt + 61_000 - Date.now()));
    }
    await expect(app.locator("#dispatch")).toBeEnabled({ timeout: 10_000 });
    const before = reloadMode ? [...loads] : await pageKeys();
    logs.push(`Round ${round} before: ${JSON.stringify(before)}`);
    // A CDP click alone does not reproduce the user's Windows focus handoff.
    execFileSync("pwsh", ["-NoProfile", "-Command", `$shell = New-Object -ComObject WScript.Shell; if (-not $shell.AppActivate(${desktop.pid})) { throw 'Could not foreground desktop' }`]);
    const startedAt = Date.now();
    await app.locator("#dispatch").click();
    await expect(app.locator("#result")).not.toContainText("正在要求", { timeout: 20_000 });
    previousDispatchAt = Date.now();
    const result = await app.locator("#result").textContent();
    logs.push(`Round ${round} at ${startedAt}, result: ${result}`);
    if (!result.startsWith("已嘗試發送")) failures.push(`Round ${round}: ${result}`);
    const roundKeys = async () => reloadMode
      ? loads.map((count, index) => count - before[index])
      : (await pageKeys()).map((keys, index) => keys.slice(before[index].length));
    try {
      await expect.poll(roundKeys, { timeout: 5000 })
        .toEqual(reloadMode ? [1, 1] : [[{ key: "F8", code: "F8", trusted: true }], [{ key: "F8", code: "F8", trusted: true }]]);
      logs.push(`PASS: round ${round}, each target ${reloadMode ? "reloaded exactly once via native Ctrl+R" : "received exactly one trusted F8"}`);
    } catch (error) {
      failures.push(`Round ${round}: ${error.message}`);
    }
    logs.push(`Round ${round} received: ${JSON.stringify(await roundKeys())}`);
  }
  assert.deepEqual(failures, [], "Every target must receive one chord in each round");
  console.log(logs.join("\n"));
} catch (error) {
  logs.push(error.stack ?? String(error));
  console.error(logs.join("\n"));
  process.exitCode = 1;
} finally {
  await finish();
}
