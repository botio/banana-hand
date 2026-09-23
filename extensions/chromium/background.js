const BROWSER_KIND = "chrome";
const NATIVE_HOST_NAME = "dev.bananahand.dispatch_host";
const INSTANCE_KEY = "browserInstanceId";
const RECONNECT_BASE_MS = 3000;
const RECONNECT_MAX_MS = 30000;
// The MV3 service worker can be reaped mid-backoff (idle workers die after
// ~30s, and a pending setTimeout does not keep the worker alive). A
// periodic alarm is the only event guaranteed to wake it, so the retry
// loop restarts at least once a minute no matter what killed the worker.
const CONNECT_WATCH_ALARM = "connect-watch";
// Chromium window/tab activation is asynchronous: tabs.update/windows.update
// resolve once the *request* is accepted, not once the target tab owns input
// focus. Reporting `ready` before that commit lets the desktop inject a chord
// into whatever tab/window is still active (on Windows this lands the first
// target's key on the previously-active target, worse in later rounds). After
// requesting activation, confirm the target is the active tab of a focused
// window (bounded poll), then give the renderer a short beat to commit focus.
const FOCUS_CONFIRM_TRIES = 20;
const FOCUS_CONFIRM_INTERVAL_MS = 80;
// After the focused/active flags flip, the renderer must still commit real
// keyboard focus (WebContents focus travels via IPC). A too-short settle lets
// the first injected chord land on the previous round's still-active tab —
// observed as "second round, target 01 attempted but nothing happens". 400ms
// is a safely conservative window, well under the desktop's 3s prepare
// timeout; the confirm loop already bounds the wait so we never exceed it.
const FOCUS_SETTLE_MS = 400;
function sleep(ms) { return new Promise((resolve) => setTimeout(resolve, ms)); }

function ensureConnectWatch() {
  chrome.alarms.create(CONNECT_WATCH_ALARM, { periodInMinutes: 1 });
}

let nativePort;
let sessionNonce = crypto.randomUUID();
let browserInstanceId;
let generation = 0;
let reconnectDelayMs = RECONNECT_BASE_MS;
let reconnectTimer;
// The browser's own diagnosis of the most recent failed connect (e.g.
// "Native messaging host not found"), reported in the next hello so the App
// can show it when no host is connected.
let lastDisconnectReason;
// Sends to the live port. nativePort can be cleared between an await and
// the post (the host dies mid-call), and some builds throw when posting on
// an already-closed port; both must stay silent, because onDisconnect
// already reschedules the retry.
function post(message) {
  if (!nativePort) return;
  try {
    nativePort.postMessage(message);
  } catch {
    // The port closed between the check and the post.
  }
}

async function ensureBrowserInstanceId() {
  const saved = await chrome.storage.local.get(INSTANCE_KEY);
  browserInstanceId = saved[INSTANCE_KEY] ?? crypto.randomUUID();
  if (!saved[INSTANCE_KEY]) await chrome.storage.local.set({ [INSTANCE_KEY]: browserInstanceId });
}

async function sendSnapshot() {
  if (!nativePort || !browserInstanceId) return;
  const windows = await chrome.windows.getAll({ populate: true });
  const tabs = windows.flatMap((window) => (window.tabs ?? [])
    .filter((tab) => Number.isInteger(tab.id) && Number.isInteger(tab.windowId))
    .map((tab) => ({
      target: {
        browser: BROWSER_KIND,
        browser_instance_id: browserInstanceId,
        session_nonce: sessionNonce,
        window_id: tab.windowId,
        tab_id: tab.id,
        generation,
      },
      title: tab.title ?? "",
      url: tab.url,
    })));
  post({
    type: "tabs_snapshot",
    request_id: crypto.randomUUID(),
    browser_instance_id: browserInstanceId,
    session_nonce: sessionNonce,
    tabs,
  });
}

async function prepareTarget(message) {
  const target = message.target;
  if (target.browser !== BROWSER_KIND
    || target.browser_instance_id !== browserInstanceId
    || target.session_nonce !== sessionNonce) {
    post({ type: "prepared", request_id: message.request_id, ready: false, code: "rejected_stale" });
    return;
  }

  try {
    const tab = await chrome.tabs.get(target.tab_id);
    if (tab.windowId !== target.window_id) throw new Error("tab 已不屬於預期視窗");
    await chrome.windows.update(target.window_id, { focused: true });
    // A focus request can complete before the window is focused. Select the
    // tab only after that handoff, so browser-level shortcuts target it.
    let windowFocused = false;
    for (let i = 0; i < FOCUS_CONFIRM_TRIES; i += 1) {
      if ((await chrome.windows.get(target.window_id)).focused) {
        windowFocused = true;
        break;
      }
      await sleep(FOCUS_CONFIRM_INTERVAL_MS);
    }
    if (!windowFocused) throw new Error("目標視窗尚未取得焦點；未激活分頁");
    await chrome.tabs.update(target.tab_id, { active: true });
    // Activating a tab in an already-frontmost window does not change the
    // foreground process, so the Windows verify_foreground gate cannot detect
    // the still-pending switch. Wait until the target tab is genuinely the
    // active tab of a focused window before acknowledging the prepare.
    let confirmed = false;
    for (let i = 0; i < FOCUS_CONFIRM_TRIES; i += 1) {
      const [currentTab, currentWindow] = await Promise.all([
        chrome.tabs.get(target.tab_id),
        chrome.windows.get(target.window_id),
      ]);
      if (currentTab.active && currentWindow.focused) {
        confirmed = true;
        break;
      }
      await sleep(FOCUS_CONFIRM_INTERVAL_MS);
    }
    if (!confirmed) {
      post({
        type: "prepared",
        request_id: message.request_id,
        ready: false,
        code: "focus_failed",
        detail: "目標分頁啟用或視窗聚焦未能在期限內確認",
      });
      return;
    }
    // The active/focused flags commit before the renderer owns keyboard
    // focus; a short beat lets the freshly-activated tab take input.
    await sleep(FOCUS_SETTLE_MS);
    post({ type: "prepared", request_id: message.request_id, ready: true });
  } catch (error) {
    post({
      type: "prepared",
      request_id: message.request_id,
      ready: false,
      code: "focus_failed",
      detail: error instanceof Error ? error.message : String(error),
    });
  }
}

function connectNativeHost() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = undefined;
  }
  // Chrome blocks a new connect while a previous port to the same host is
  // still open; closing it first keeps every retry from deadlocking.
  if (nativePort) {
    nativePort.disconnect();
    nativePort = undefined;
  }
  const port = chrome.runtime.connectNative(NATIVE_HOST_NAME);
  nativePort = port;
  port.onMessage.addListener((message) => {
    if (nativePort !== port) return;
    // Any message from the host proves the bridge is alive; reset backoff so a
    // later drop retries quickly.
    reconnectDelayMs = RECONNECT_BASE_MS;
    if (message.type === "error") {
      // The handshake was rejected (stale capability token after an app
      // restart, or a protocol mismatch). Tearing the port down makes the
      // retry loop relaunch the host, which re-reads the fresh bridge.json.
      // Local disconnect does not fire this port's onDisconnect event.
      nativePort = undefined;
      port.disconnect();
      scheduleReconnect();
      return;
    }
    lastDisconnectReason = undefined;
    if (message.type === "prepare") void prepareTarget(message);
  });
  port.onDisconnect.addListener(() => {
    if (nativePort !== port) return;
    nativePort = undefined;
    if (chrome.runtime.lastError) {
      lastDisconnectReason = String(chrome.runtime.lastError.message ?? chrome.runtime.lastError);
      // The App only learns the reason on the next hello, which never comes
      // when the port never connected — so also log it where the user can
      // find it: chrome://extensions → Banana Hand → service worker console.
      console.warn("[banana-hand] native host connect failed:", lastDisconnectReason);
    }
    scheduleReconnect();
  });
  post({
    type: "hello",
    request_id: crypto.randomUUID(),
    protocol_major: 1,
    browser: BROWSER_KIND,
    browser_instance_id: browserInstanceId,
    session_nonce: sessionNonce,
    last_disconnect_reason: lastDisconnectReason,
  });
  void sendSnapshot();
}

function scheduleReconnect() {
  // The desktop app may not be running yet, or it may have restarted and
  // rotated its capability token. Retrying with backoff means the order in
  // which the app and the browser are started no longer matters.
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = undefined;
    if (!nativePort) connectNativeHost();
  }, reconnectDelayMs);
  reconnectDelayMs = Math.min(reconnectDelayMs * 2, RECONNECT_MAX_MS);
}

function scheduleSnapshot() {
  generation += 1;
  void sendSnapshot();
}
chrome.tabs.onCreated.addListener(scheduleSnapshot);
chrome.tabs.onRemoved.addListener(scheduleSnapshot);
chrome.tabs.onReplaced.addListener(scheduleSnapshot);
chrome.tabs.onUpdated.addListener(scheduleSnapshot);
chrome.tabs.onAttached.addListener(scheduleSnapshot);
chrome.tabs.onDetached.addListener(scheduleSnapshot);
chrome.windows.onRemoved.addListener(scheduleSnapshot);
chrome.runtime.onStartup.addListener(() => {
  sessionNonce = crypto.randomUUID();
  lastDisconnectReason = undefined;
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = undefined;
  }
  ensureConnectWatch();
  void ensureBrowserInstanceId().then(connectNativeHost);
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name !== CONNECT_WATCH_ALARM || nativePort) return;
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = undefined;
  }
  reconnectDelayMs = RECONNECT_BASE_MS;
  void ensureBrowserInstanceId().then(connectNativeHost);
});

ensureConnectWatch();
void ensureBrowserInstanceId().then(connectNativeHost);
