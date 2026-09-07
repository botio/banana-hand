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
const server = createServer((request, response) => {
  const name = request.url === "/second" ? "second" : "first";
  response.setHeader("Content-Type", "text/html; charset=utf-8");
  response.end(`<!doctype html><title>Banana smoke ${name}</title><h1>${name}</h1><output id="keys">0</output><script>
    window.receivedKeys = [];
    addEventListener('keydown', event => {
      if (event.code === 'F8') {
        event.preventDefault();
        receivedKeys.push({code:event.code, trusted:event.isTrusted});
        document.querySelector('#keys').textContent = receivedKeys.length;
      }
    });
  </script>`);
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
const watchdog = setTimeout(() => {
  logs.push("FAIL: browser smoke exceeded 90 seconds");
  void finish().finally(() => process.exit(1));
}, 90_000);

async function finish() {
  clearTimeout(watchdog);
  if (app) {
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
  browserContext = await chromium.launchPersistentContext(path.join(temporary, "chromium"), {
    channel: "chromium",
    headless: false,
    args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`],
  });
  const worker = browserContext.serviceWorkers()[0] ?? await browserContext.waitForEvent("serviceworker");
  worker.on("console", message => logs.push(`extension: ${message.text()}`));
  const first = await browserContext.newPage();
  const second = await browserContext.newPage();
  await first.goto(`${origin}/first`);
  await second.goto(`${origin}/second`);
  await expect(app.locator("#first-target option").filter({ hasText: "Banana smoke first" })).toHaveCount(1, { timeout: 30_000 });
  const firstValue = await app.locator("#first-target option").filter({ hasText: "Banana smoke first" }).getAttribute("value");
  const secondValue = await app.locator("#second-target option").filter({ hasText: "Banana smoke second" }).getAttribute("value");
  assert.ok(firstValue && secondValue);
  logs.push("PASS: real extension hello and both tab snapshots reached installed desktop");

  await app.getByRole("textbox", { name: "快捷鍵名稱" }).fill("Windows smoke F8");
  await app.getByRole("button", { name: "快捷鍵組合" }).click();
  await app.keyboard.press("F8");
  await app.getByRole("button", { name: "新增快捷鍵" }).click();
  await app.locator("#first-target").selectOption(firstValue);
  await app.locator("#second-target").selectOption(secondValue);
  await expect(app.locator("#dispatch")).toBeEnabled();
  // Make the actual desktop window foreground before its synchronous command
  // runs; a CDP tab click alone does not reproduce Windows focus handoff.
  execFileSync("pwsh", ["-NoProfile", "-Command", `$shell = New-Object -ComObject WScript.Shell; if (-not $shell.AppActivate(${desktop.pid})) { throw 'Could not foreground desktop' }`]);
  await app.locator("#dispatch").click();
  await expect(app.locator("#result")).not.toContainText("正在要求", { timeout: 20_000 });
  const result = await app.locator("#result").textContent();
  logs.push(`Dispatch result: ${result}`);
  assert.ok(!result.includes("逾時"), `Foreground preparation timed out: ${result}`);
  assert.ok(result.startsWith("已嘗試發送"), `Dispatch rejected: ${result}`);
  await expect.poll(() => first.evaluate(() => receivedKeys), { timeout: 5000 }).toEqual([{ code: "F8", trusted: true }]);
  await expect.poll(() => second.evaluate(() => receivedKeys), { timeout: 5000 }).toEqual([{ code: "F8", trusted: true }]);
  logs.push("PASS: both real Chromium tabs observed exactly one trusted F8 from installed desktop dispatch");
  console.log(logs.join("\n"));
} catch (error) {
  logs.push(error.stack ?? String(error));
  console.error(logs.join("\n"));
  process.exitCode = 1;
} finally {
  await finish();
}
