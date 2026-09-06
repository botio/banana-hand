const BROWSER_KIND = "firefox";
const NATIVE_HOST_NAME = "dev.bananahand.dispatch_host";
const INSTANCE_KEY = "browserInstanceId";
const RECONNECT_BASE_MS = 3000;
const RECONNECT_MAX_MS = 30000;
// An idle event page can lose setTimeout retries; alarms wake it again.
const CONNECT_WATCH_ALARM = "connect-watch";

function ensureConnectWatch() {
  browser.alarms.create(CONNECT_WATCH_ALARM, { periodInMinutes: 1 });
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
  const saved = await browser.storage.local.get(INSTANCE_KEY);
  browserInstanceId = saved[INSTANCE_KEY] ?? crypto.randomUUID();
  if (!saved[INSTANCE_KEY]) await browser.storage.local.set({ [INSTANCE_KEY]: browserInstanceId });
}

async function sendSnapshot() {
  if (!nativePort || !browserInstanceId) return;
  const windows = await browser.windows.getAll({ populate: true });
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
    const tab = await browser.tabs.get(target.tab_id);
    if (tab.windowId !== target.window_id) throw new Error("tab 已不屬於預期視窗");
    await browser.tabs.update(target.tab_id, { active: true });
    await browser.windows.update(target.window_id, { focused: true });
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
  // Some browser builds block a new connect while a previous port to the
  // same host is still open; closing it first keeps every retry from
  // deadlocking.
  if (nativePort) {
    nativePort.disconnect();
    nativePort = undefined;
  }
  const port = browser.runtime.connectNative(NATIVE_HOST_NAME);
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
    if (port.error) {
      lastDisconnectReason = String(port.error.message ?? port.error);
    }
    console.warn("[banana-hand] native host connect failed:", lastDisconnectReason);
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

browser.tabs.onCreated.addListener(scheduleSnapshot);
browser.tabs.onRemoved.addListener(scheduleSnapshot);
browser.tabs.onReplaced.addListener(scheduleSnapshot);
browser.tabs.onUpdated.addListener(scheduleSnapshot);
browser.tabs.onAttached.addListener(scheduleSnapshot);
browser.tabs.onDetached.addListener(scheduleSnapshot);
browser.windows.onRemoved.addListener(scheduleSnapshot);
browser.runtime.onStartup.addListener(() => {
  sessionNonce = crypto.randomUUID();
  lastDisconnectReason = undefined;
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = undefined;
  }
  ensureConnectWatch();
  void ensureBrowserInstanceId().then(connectNativeHost);
});

browser.alarms.onAlarm.addListener((alarm) => {
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
