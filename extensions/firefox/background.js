const BROWSER_KIND = "firefox";
const NATIVE_HOST_NAME = "dev.bananahand.dispatch_host";
const INSTANCE_KEY = "browserInstanceId";

let nativePort;
let sessionNonce = crypto.randomUUID();
let browserInstanceId;
let generation = 0;

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
    const tab = await browser.tabs.get(target.tab_id);
    if (tab.windowId !== target.window_id) throw new Error("tab 已不屬於預期視窗");
    await browser.tabs.update(target.tab_id, { active: true });
    await browser.windows.update(target.window_id, { focused: true });
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
  nativePort = browser.runtime.connectNative(NATIVE_HOST_NAME);
  nativePort.onDisconnect.addListener(() => { nativePort = undefined; });
  nativePort.onMessage.addListener((message) => {
    if (message.type === "prepare") void prepareTarget(message);
  });
  nativePort.postMessage({
    type: "hello",
    request_id: crypto.randomUUID(),
    protocol_major: 1,
    browser: BROWSER_KIND,
    browser_instance_id: browserInstanceId,
    session_nonce: sessionNonce,
  });
  void sendSnapshot();
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
  void ensureBrowserInstanceId().then(connectNativeHost);
});

void ensureBrowserInstanceId().then(connectNativeHost);
