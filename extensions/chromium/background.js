const BROWSER_KIND = "chrome";
const NATIVE_HOST_NAME = "dev.bananahand.dispatch_host";
const INSTANCE_KEY = "browserInstanceId";
const RECONNECT_BASE_MS = 3000;
const RECONNECT_MAX_MS = 30000;

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
  nativePort.postMessage({
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
    nativePort.postMessage({ type: "prepared", request_id: message.request_id, ready: false, code: "rejected_stale" });
    return;
  }

  try {
    const tab = await chrome.tabs.get(target.tab_id);
    if (tab.windowId !== target.window_id) throw new Error("tab 已不屬於預期視窗");
    await chrome.tabs.update(target.tab_id, { active: true });
    await chrome.windows.update(target.window_id, { focused: true });
    nativePort.postMessage({ type: "prepared", request_id: message.request_id, ready: true });
  } catch (error) {
    nativePort.postMessage({
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
    // Any message from the host proves the bridge is alive; reset backoff so a
    // later drop retries quickly.
    reconnectDelayMs = RECONNECT_BASE_MS;
    if (message.type === "error") {
      // The handshake was rejected (stale capability token after an app
      // restart, or a protocol mismatch). Tearing the port down makes the
      // retry loop relaunch the host, which re-reads the fresh bridge.json.
      port.disconnect();
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
    }
    scheduleReconnect();
  });
  port.postMessage({
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
  void ensureBrowserInstanceId().then(connectNativeHost);
});

void ensureBrowserInstanceId().then(connectNativeHost);
