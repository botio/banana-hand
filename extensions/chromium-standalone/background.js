import { normalizeChord, parseChord } from "./keyboard.js";
import { dispatchDomChord } from "./dispatch-dom.js";

const chrome = globalThis.browser ?? globalThis.chrome;
const ICON = { 16: "icons/icon-16.png", 32: "icons/icon-32.png", 48: "icons/icon-48.png", 64: "icons/icon-64.png", 128: "icons/icon-128.png" };
void chrome.action.setIcon({ path: ICON });

const SHORTCUTS_KEY = "standaloneShortcuts";
const LOCK_KEY = "standaloneDispatchLock";
const TARGETS_KEY = "standaloneTargets";
const RESULT_KEY = "standaloneLastResult";
const COOLDOWN_MS = 15_000;
const DEFAULT_SHORTCUTS = [
  { id: "buy", name: "Tradovate 買入快捷鍵（僅發送）", chord: "Shift+B" },
  { id: "sell", name: "Tradovate 賣出快捷鍵（僅發送）", chord: "Shift+S" },
];

let targets = [null, null];
let shortcuts = [];
let cooldownUntil = 0;
let busy = false;
let lastResult = null;
let initializationError;
let mutationQueue = Promise.resolve();
let operation;
let focusGuard;

const errorText = (error) => error instanceof Error ? error.message : String(error);
const sleep = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));
const validId = (value) => Number.isInteger(value) && value >= 0;

function eligibleTab(tab) {
  return validId(tab.id) && typeof tab.url === "string"
    && /^https?:\/\//i.test(tab.url);
}

function validateTargetIds(value, requireBoth = false) {
  if (!Array.isArray(value) || value.length !== 2
    || value.some((id) => id !== null && !validId(id))) {
    throw new Error("目標必須是兩個有效的分頁 ID 或未選取值。");
  }
  if (requireBoth && value.some((id) => id === null)) throw new Error("請先選取兩個目標分頁。");
  if (value[0] !== null && value[0] === value[1]) throw new Error("兩個目標必須是不同的分頁。");
  return [...value];
}

function validateShortcuts(value) {
  if (!Array.isArray(value) || value.length > 50) throw new Error("快捷鍵庫最多可有 50 組快捷鍵。");
  const ids = new Set();
  return value.map((entry) => {
    if (!entry || typeof entry !== "object" || typeof entry.id !== "string"
      || !/^[A-Za-z0-9_-]{1,80}$/.test(entry.id) || ids.has(entry.id)) {
      throw new Error("快捷鍵 ID 必須唯一，且為 1–80 個英數字、底線或連字號。");
    }
    if (typeof entry.name !== "string" || !entry.name.trim() || entry.name.trim().length > 48) {
      throw new Error("快捷鍵名稱須為 1–48 個字元。");
    }
    ids.add(entry.id);
    return { id: entry.id, name: entry.name.trim(), chord: normalizeChord(entry.chord) };
  });
}

async function initialize() {
  const [local, session] = await Promise.all([
    chrome.storage.local.get([SHORTCUTS_KEY, LOCK_KEY]),
    chrome.storage.session.get([TARGETS_KEY, RESULT_KEY]),
  ]);
  shortcuts = validateShortcuts(local[SHORTCUTS_KEY] ?? DEFAULT_SHORTCUTS);
  targets = validateTargetIds(session[TARGETS_KEY] ?? [null, null]);
  lastResult = session[RESULT_KEY] ?? null;
  const lock = local[LOCK_KEY];
  if (lock !== undefined) {
    if (!lock || typeof lock.busy !== "boolean" || !Number.isFinite(lock.cooldownUntil)
      || lock.cooldownUntil < 0) throw new Error("持久冷卻鎖定資料無效；已停止發送，請檢查擴充功能儲存空間。");
    cooldownUntil = lock.cooldownUntil;
    if (lock.busy) {
      // Selection is immutable while busy. The previous result may still
      // describe an older dispatch if termination preceded its session write.
      const interruptedTargets = targets.filter(validId);
      lastResult = {
        targets: interruptedTargets.map((tabId) => ({
          tabId,
          status: "failed",
          detail: "背景工作中斷，是否曾送出按鍵無法確認；不會自動重送，請人工確認頁面狀態。",
        })),
        detail: "上次發送中斷，結果未知。已保留原有冷卻時間；不會重播或聲稱下單成功。",
      };
      await chrome.storage.session.set({ [RESULT_KEY]: lastResult });
      await chrome.storage.local.set({ [LOCK_KEY]: { busy: false, cooldownUntil } });
    }
  }
  if (local[SHORTCUTS_KEY] === undefined) {
    await chrome.storage.local.set({ [SHORTCUTS_KEY]: shortcuts });
  }
}

const ready = initialize().catch((error) => {
  initializationError = `無法初始化獨立試用版：${errorText(error)}`;
});

function serializeMutation(callback) {
  const result = mutationQueue.then(callback);
  mutationQueue = result.catch(() => {});
  return result;
}

async function state() {
  const tabs = (await chrome.tabs.query({})).filter(eligibleTab).map((tab) => ({
    id: tab.id, windowId: tab.windowId, title: tab.title || `分頁 ${tab.id}`, url: tab.url,
  }));
  return { ok: true, tabs, targets: [...targets], shortcuts: shortcuts.map((entry) => ({ ...entry })), cooldownUntil, busy, lastResult };
}

async function getLiveTargets(ids, dispatch = false) {
  validateTargetIds(ids, dispatch);
  return Promise.all(ids.map(async (id) => {
    if (id === null) return null;
    const tab = await chrome.tabs.get(id);
    if (!eligibleTab(tab)) throw new Error(`分頁 ${id} 不支援；請選擇一般網頁，勿選擇系統或擴充功能頁面。`);
    if (dispatch && (tab.discarded || tab.status === "loading" || tab.pendingUrl)) {
      throw new Error(`分頁 ${id} 尚未就緒或正在導覽；本次不會發送。`);
    }
    if (dispatch) {
      const pattern = `${new URL(tab.url).origin}/*`;
      if (!(await chrome.permissions.contains({ origins: [pattern] }))) {
        throw new Error(`尚未允許 ${pattern}；請由傳送按鈕重新授權，本次不會發送。`);
      }
    }
    return tab;
  }));
}

async function configure(message) {
  if (busy) throw new Error("正在發送或鎖定狀態未確認，無法變更設定。");
  const nextTargets = validateTargetIds(message.targets ?? targets);
  const nextShortcuts = validateShortcuts(message.shortcuts ?? shortcuts);
  if (Object.hasOwn(message, "targets") && message.targets == null) throw new Error("目標設定格式無效。");
  if (Object.hasOwn(message, "shortcuts") && message.shortcuts == null) throw new Error("快捷鍵庫格式無效。");
  await getLiveTargets(nextTargets);
  // Validate every field before either storage area is changed.
  if (Object.hasOwn(message, "shortcuts")) {
    await chrome.storage.local.set({ [SHORTCUTS_KEY]: nextShortcuts });
    shortcuts = nextShortcuts;
  }
  if (Object.hasOwn(message, "targets")) {
    await chrome.storage.session.set({ [TARGETS_KEY]: nextTargets });
    targets = nextTargets;
  }
  return state();
}


function assertOperation() {
  if (operation?.interrupted) throw new Error(operation.interrupted);
  if (focusGuard?.interrupted) throw new Error(focusGuard.interrupted);
}

async function revalidateTargets(snapshots) {
  assertOperation();
  const current = await getLiveTargets(snapshots.map((tab) => tab.id), true);
  for (let index = 0; index < snapshots.length; index += 1) {
    if (current[index].windowId !== snapshots[index].windowId || current[index].url !== snapshots[index].url) {
      throw new Error(`分頁 ${snapshots[index].id} 的位置或網址已變更；已停止後續發送。`);
    }
  }
  assertOperation();
}

async function assertFocused(target, documentFocus = true) {
  assertOperation();
  const [tab, window] = await Promise.all([chrome.tabs.get(target.id), chrome.windows.get(target.windowId)]);
  if (!tab.active || !window.focused || tab.windowId !== target.windowId || tab.url !== target.url
    || tab.status === "loading" || tab.pendingUrl || tab.discarded) {
    throw new Error("目標已失去焦點、移動或導覽；不會搶回焦點或繼續發送。");
  }
  if (documentFocus) {
    const responses = await chrome.scripting.executeScript({
      target: { tabId: target.id },
      func: () => document.hasFocus(),
    });
    if (responses[0]?.result !== true) {
      throw new Error("頁面本身未持有鍵盤焦點；本次停止發送。");
    }
  }
  assertOperation();
}

async function acquireFocus(target) {
  assertOperation();
  focusGuard = { tabId: target.id, windowId: target.windowId, interrupted: null };
  // Exactly one focus request per target. Polling only observes the handoff;
  // it never re-activates a tab after a user changes focus.
  await chrome.windows.update(target.windowId, { focused: true });
  assertOperation();
  await chrome.tabs.update(target.id, { active: true });
  let confirmed = false;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    assertOperation();
    const [tab, window] = await Promise.all([chrome.tabs.get(target.id), chrome.windows.get(target.windowId)]);
    if (tab.active && window.focused) {
      confirmed = true;
      break;
    }
    await sleep(50);
  }
  if (!confirmed) throw new Error("目標分頁或視窗未能取得焦點；未發送按鍵。");
  // Match the bridge's renderer-focus settling interval, not its retry logic.
  await sleep(400);
  await assertFocused(target);
}

async function dispatchTarget(target, chord) {
  let began = false;
  try {
    await acquireFocus(target);
    await assertFocused(target);
    // Mark uncertainty before injection: a lost response must never be replayed.
    began = true;
    const results = await chrome.scripting.executeScript({
      target: { tabId: target.id, allFrames: true },
      func: dispatchDomChord,
      args: [chord],
    });
    const sent = results.filter((entry) => entry.result?.attempted);
    if (sent.length !== 1 || sent[0].result.error) {
      throw new Error(sent[0]?.result?.error || "無法確認唯一的聚焦頁框已接收事件；不會重送。");
    }
    assertOperation();
    return { tabId: target.id, status: "attempted", detail: "已派送合成按鍵事件（isTrusted=false）；網站可能忽略，不代表訂單成立或成交。" };
  } catch (error) {
    return { tabId: target.id, status: "failed", detail: `${began ? "可能已派送部分事件，結果未知；不會重送。" : "未派送事件。"}${errorText(error)}` };
  }
}

async function finishDispatch(result) {
  lastResult = result;
  // Keep the durable busy bit until the final per-target result is saved.
  // Failure leaves the worker fail-closed; restart reports an unknown result.
  try {
    await chrome.storage.session.set({ [RESULT_KEY]: result });
    await chrome.storage.local.set({ [LOCK_KEY]: { busy: false, cooldownUntil } });
    busy = false;
  } catch (error) {
    result.detail += ` 結果／鎖定儲存失敗：${errorText(error)}。已保持鎖定；請勿假設未送出，請人工確認後重新載入擴充功能。`;
  }
  operation = undefined;
  focusGuard = undefined;
  return result;
}

async function performDispatch(snapshots, chord, result) {
  operation = { ids: snapshots.map((tab) => tab.id), interrupted: null };
  try {
    for (let index = 0; index < snapshots.length; index += 1) {
      // Re-check BOTH targets before each attempt, including before target 2.
      await revalidateTargets(snapshots);
      if (index > 0) await assertFocused(snapshots[index - 1], false);
      result.targets[index] = await dispatchTarget(snapshots[index], chord);
      lastResult = result;
      await chrome.storage.session.set({ [RESULT_KEY]: result });
      if (result.targets[index].status !== "attempted") break;
    }
    result.detail = result.targets.every((entry) => entry.status === "attempted")
      ? "兩個分頁均已嘗試發送；未確認頁面接受、訂單建立或成交。請自行檢查兩邊結果。"
      : "發送未全部完成；未嘗試的目標不會補送，也不會自動重試。請自行確認兩邊結果。";
  } catch (error) {
    result.detail = `已停止後續發送：${errorText(error)} 不會自動重試；請自行確認是否已有按鍵或訂單生效。`;
  }
  for (const entry of result.targets) {
    if (entry.status === "not_attempted") entry.detail = "流程已停止，此目標未嘗試發送；不會自動補送。";
  }
  return finishDispatch(result);
}

async function beginDispatch(shortcutId) {
  if (busy) throw new Error("已有發送進行中或鎖定未確認；不接受重複發送。");
  if (Date.now() < cooldownUntil) throw new Error("仍在 15 秒全域冷卻期間，請等待倒數結束。");
  const shortcut = shortcuts.find((entry) => entry.id === shortcutId);
  if (!shortcut) throw new Error("找不到指定的快捷鍵，請重新選取。");
  const chord = parseChord(shortcut.chord);
  const snapshots = await getLiveTargets(validateTargetIds(targets, true), true);
  const result = {
    targets: snapshots.map((tab) => ({ tabId: tab.id, status: "not_attempted", detail: "尚未嘗試發送。" })),
    detail: `正在依序發送 ${shortcut.chord}；不代表下單或成交確認。`,
  };
  busy = true;
  cooldownUntil = Date.now() + COOLDOWN_MS;
  lastResult = result;
  // Persist the shared lock and cooldown BEFORE focus or keyboard events.
  try {
    await chrome.storage.local.set({ [LOCK_KEY]: { busy: true, cooldownUntil } });
  } catch (error) {
    result.detail = `無法確認冷卻鎖定已儲存，本次未送出按鍵；已保持鎖定：${errorText(error)}`;
    throw new Error(result.detail);
  }
  try {
    await chrome.storage.session.set({ [RESULT_KEY]: result });
  } catch (error) {
    result.detail = `無法儲存本次發送狀態，未送出按鍵：${errorText(error)}`;
    return { completion: finishDispatch(result) };
  }
  // Do not keep the mutation queue behind the dispatch. State polling remains
  // responsive and queued configure/dispatch calls see busy=true and reject.
  return { completion: performDispatch(snapshots, chord, result) };
}

async function handleMessage(message, sender) {
  if (sender.id !== chrome.runtime.id) throw new Error("拒絕非本擴充功能的要求。");
  await ready;
  if (initializationError) throw new Error(initializationError);
  if (!message || typeof message !== "object") throw new Error("要求格式無效。");
  switch (message.type) {
    case "state": return state();
    case "configure":
      if (busy) throw new Error("正在發送或鎖定未確認，無法變更設定。");
      return serializeMutation(() => configure(message));
    case "dispatch": {
      if (busy) throw new Error("已有發送進行中或鎖定未確認；不接受重複發送。");
      const job = await serializeMutation(() => beginDispatch(message.shortcutId));
      return { ok: true, result: await job.completion };
    }
    default: throw new Error("不支援的要求類型。");
  }
}

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  handleMessage(message, sender).then(sendResponse, (error) => sendResponse({ ok: false, error: errorText(error) }));
  return true;
});

chrome.tabs.onActivated.addListener(({ tabId, windowId }) => {
  if (focusGuard && windowId === focusGuard.windowId && tabId !== focusGuard.tabId) {
    focusGuard.interrupted = "目標視窗的作用中分頁已變更；不會搶回焦點。";
  }
});
chrome.windows.onFocusChanged.addListener((windowId) => {
  if (focusGuard && windowId !== focusGuard.windowId) focusGuard.interrupted = "視窗焦點已變更；不會搶回焦點。";
});
chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (operation?.ids.includes(tabId) && (changeInfo.status === "loading" || changeInfo.url !== undefined)) {
    operation.interrupted = `目標分頁 ${tabId} 開始導覽；已停止後續發送。`;
  }
});
chrome.tabs.onRemoved.addListener((tabId) => {
  if (operation?.ids.includes(tabId)) operation.interrupted = `目標分頁 ${tabId} 已關閉。`;
});
chrome.tabs.onReplaced.addListener((_addedTabId, removedTabId) => {
  if (operation?.ids.includes(removedTabId)) operation.interrupted = `目標分頁 ${removedTabId} 已被替換。`;
});
chrome.tabs.onDetached.addListener((tabId) => {
  if (operation?.ids.includes(tabId)) operation.interrupted = `目標分頁 ${tabId} 已移動至其他視窗。`;
});

chrome.action.onClicked.addListener(() => {
  void chrome.tabs.create({ url: chrome.runtime.getURL("panel.html") });
});
