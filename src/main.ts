import { invoke } from "@tauri-apps/api/core";

import type {
  DispatchOutcome,
  NativeHostRegistrationResult,
  RuntimeSnapshot,
  Settings,
  ShortcutRecord,
  TabMetadata,
} from "./domain";
import { SettingsRepository } from "./settings";
import "./styles.css";

const root = document.querySelector<HTMLElement>("#app");
if (!root) throw new Error("缺少 #app 節點");
const app: HTMLElement = root;

let settings: Settings = { schemaVersion: 1, shortcuts: [] };
let renderedShortcuts: ShortcutRecord[] | undefined;
let savingShortcut = false;
let snapshot: RuntimeSnapshot = {
  tabs: [],
  cooldown_remaining_seconds: 0,
  connected_hosts: 0,
  last_bridge_rejection: null,
};
let selectedShortcutId: string | undefined;
let selectedFirstTarget: string | undefined;
let selectedSecondTarget: string | undefined;
let repository: SettingsRepository | undefined;
let statusMessage = "正在連線到桌面協調器…";
let nativeHostBrowser: "chrome" | "firefox" = "chrome";
let nativeHostResult = "";
let dispatchResult = "";
const MODIFIER_NAME_BY_INPUT: Record<string, string> = {
  ctrl: "Ctrl",
  alt: "Alt",
  shift: "Shift",
  meta: "Meta",
};

const NAMED_KEY_BY_INPUT: Record<string, string> = {
  Esc: "Esc",
  Enter: "Enter",
  Tab: "Tab",
  Space: "Space",
};


function targetKey(tab: TabMetadata): string {
  const target = tab.target;
  return [
    target.browser,
    target.browser_instance_id,
    target.session_nonce,
    target.window_id,
    target.tab_id,
  ].join(":");
}

function formatTab(tab: TabMetadata): string {
  const target = tab.target;
  return `${target.browser} · 視窗 ${target.window_id} · ${tab.title || `分頁 ${target.tab_id}`}`;
}

function normalizeChord(raw: string): string {
  const segments = raw.split("+").map((segment) => segment.trim()).filter(Boolean);
  if (!segments.length) throw new Error("快捷鍵不可為空白。");
  const normalized = segments.map((segment, index) => {
    if (index < segments.length - 1) {
      const modifier = MODIFIER_NAME_BY_INPUT[segment.toLowerCase()];
      if (!modifier) throw new Error(`不支援的修飾鍵：${segment}`);
      return modifier;
    }
    if (/^f([1-9]|1\d|2[0-4])$/i.test(segment)) return segment.toUpperCase();
    if (/^[a-z0-9]$/i.test(segment)) return segment.toUpperCase();
    if (NAMED_KEY_BY_INPUT[segment]) return NAMED_KEY_BY_INPUT[segment];
    throw new Error(`不支援的主要按鍵：${segment}`);
  });
  if (new Set(normalized.slice(0, -1)).size !== normalized.length - 1) {
    throw new Error("修飾鍵不可重複。");
  }
  return normalized.join("+");
}

const CHORD_RECORDING_HINT = "請按下組合鍵；Esc 取消。";

let chordRecorder: HTMLButtonElement | undefined;
let chordValue: HTMLSpanElement | undefined;
let chordHint: HTMLSpanElement | undefined;
let chordHiddenInput: HTMLInputElement | undefined;
let capturingChord = false;

function mainKeyFromEvent(event: KeyboardEvent):
  | { name: string; kind: "character" | "function" | "named" }
  | null {
  // `event.code` is layout-independent (physical key), matching how the
  // native input adapter emits chords.
  if (/^Key[A-Z]$/.test(event.code)) return { name: event.code.slice(3), kind: "character" };
  if (/^Digit[0-9]$/.test(event.code)) return { name: event.code.slice(5), kind: "character" };
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(event.code)) return { name: event.code, kind: "function" };
  switch (event.key) {
    case "Enter": return { name: "Enter", kind: "named" };
    case "Tab": return { name: "Tab", kind: "named" };
    case " ": return { name: "Space", kind: "named" };
    case "Escape": return { name: "Esc", kind: "named" };
    default: return null;
  }
}

function setChord(chord: string | null): void {
  if (!chordHiddenInput || !chordValue) return;
  chordHiddenInput.value = chord ?? "";
  chordValue.textContent = chord ?? "請按下快捷鍵組合…";
  chordRecorder?.classList.toggle("has-value", Boolean(chord));
}

function beginChordCapture(): void {
  if (capturingChord || !chordRecorder || !chordHint) return;
  capturingChord = true;
  chordHint.textContent = CHORD_RECORDING_HINT;
  chordRecorder.classList.add("capturing");
  window.addEventListener("keydown", onChordKeyDown, true);
}

function endChordCapture(): void {
  if (!capturingChord) return;
  capturingChord = false;
  chordRecorder?.classList.remove("capturing");
  window.removeEventListener("keydown", onChordKeyDown, true);
}

function onChordKeyDown(event: KeyboardEvent): void {
  if (!capturingChord) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "Escape"
    && !event.ctrlKey && !event.shiftKey && !event.altKey && !event.metaKey) {
    endChordCapture();
    if (chordHint) chordHint.textContent = "";
    return;
  }
  const main = mainKeyFromEvent(event);
  if (!main) {
    if (chordHint) chordHint.textContent = "不支援的主要按鍵；請改按 A-Z、0-9、F1-F24、Esc、Enter、Tab 或 Space。";
    return;
  }
  const modifiers = [
    event.ctrlKey ? "Ctrl" : undefined,
    event.shiftKey ? "Shift" : undefined,
    event.altKey ? "Alt" : undefined,
    event.metaKey ? "Meta" : undefined,
  ].filter((modifier): modifier is string => modifier !== undefined);
  if (!modifiers.length && main.kind !== "function") {
    if (chordHint) chordHint.textContent = "主要按鍵需搭配至少一個修飾鍵（Ctrl / Shift / Alt / Meta）。";
    return;
  }
  const chord = [...modifiers, main.name].join("+");
  setChord(normalizeChord(chord));
  endChordCapture();
  if (chordHint) chordHint.textContent = "";
}

function selectedShortcut(): ShortcutRecord | undefined {
  return settings.shortcuts.find((shortcut) => shortcut.id === selectedShortcutId);
}

function selectedTab(key: string | undefined): TabMetadata | undefined {
  return snapshot.tabs.find((tab) => targetKey(tab) === key);
}

function canDispatch(): boolean {
  return Boolean(
    selectedShortcut()
      && selectedFirstTarget
      && selectedSecondTarget
      && selectedFirstTarget !== selectedSecondTarget
      && snapshot.cooldown_remaining_seconds === 0,
  );
}

function mount(): void {
  app.innerHTML = `
    <section class="shell" aria-label="Banana Hand 發送協調器">
      <header class="masthead">
        <div>
          <p class="eyebrow">BANANA HAND / DESKTOP COORDINATOR</p>
          <h1>一次發送，兩個目標。</h1>
        </div>
        <p id="connection-status" class="connection-status"></p>
      </header>
      <div class="workbench">
        <section class="panel shortcut-panel" aria-labelledby="shortcut-title">
          <div class="panel-heading">
            <div>
              <p class="eyebrow">PERSISTENT SETTINGS</p>
              <h2 id="shortcut-title">快捷鍵庫</h2>
            </div>
            <span id="shortcut-count"></span>
          </div>
          <div id="shortcut-list" class="shortcut-list" role="radiogroup" aria-label="選取本次發送的快捷鍵"></div>
          <form id="shortcut-form" class="shortcut-form">
            <label>快捷鍵名稱<input name="name" required maxlength="48" placeholder="例如：部署確認" /></label>
            <label>快捷鍵組合
              <button type="button" class="chord-recorder" id="chord-recorder" aria-label="快捷鍵組合">
                <span id="chord-recorder-value">請按下快捷鍵組合…</span>
              </button>
              <input name="chord" type="hidden" />
              <span class="chord-recorder-hint" id="chord-recorder-hint" role="status"></span>
            </label>
            <button type="submit">新增快捷鍵</button>
          </form>
        </section>
        <section class="panel dispatch-panel" aria-labelledby="dispatch-title">
          <div class="panel-heading">
            <div>
              <p class="eyebrow">SESSION-ONLY TARGETS</p>
              <h2 id="dispatch-title">發送</h2>
            </div>
            <span id="cooldown" class="cooldown"></span>
          </div>
          <p class="contract">兩個目標須為不同的已連線 Browser Tab。發送是盡力嘗試，不保證 trusted、原子或送達。</p>
          <div class="targets">
            <label>目標 01<select id="first-target"><option value="">重新選擇目標</option></select></label>
            <label>目標 02<select id="second-target"><option value="">重新選擇目標</option></select></label>
          </div>
          <div class="proof" id="proof"></div>
          <button id="dispatch" class="dispatch-button" type="button">發送快捷鍵</button>
          <p id="result" class="result" role="status"></p>
        </section>
      </div>
      <section class="panel native-host-panel" aria-labelledby="native-host-title">
        <div class="panel-heading">
          <div>
            <p class="eyebrow">NATIVE MESSAGING HOST</p>
            <h2 id="native-host-title">讓 Browser 找到 Host</h2>
          </div>
        </div>
        <p class="contract">
          Browser 透過 native messaging 機制啟動 native host；native host 再經 OS-user IPC
          連回桌面 App。選擇 browser 後登錄 native host，extension 連線後會自動回報可選分頁。
          extension 斷線後會自動重試（3–30 秒間隔），App 與 browser 的啟動順序不再重要。
        </p>
        <div class="native-host-form">
          <label>
            Browser
            <select id="nh-browser">
              <option value="chrome">Chrome / Chromium</option>
              <option value="firefox">Firefox</option>
            </select>
          </label>
          <button id="nh-register" class="dispatch-button" type="button">登錄 native host</button>
        </div>
        <p id="nh-result" class="result" role="status"></p>
      </section>
    </section>
  `;

  requireElement<HTMLButtonElement>("#dispatch").addEventListener("click", dispatchShortcut);
  const firstTarget = requireElement<HTMLSelectElement>("#first-target");
  firstTarget.addEventListener("change", () => {
    selectedFirstTarget = firstTarget.value || undefined;
    render();
  });
  const secondTarget = requireElement<HTMLSelectElement>("#second-target");
  secondTarget.addEventListener("change", () => {
    selectedSecondTarget = secondTarget.value || undefined;
    render();
  });
  const nativeHostSelect = requireElement<HTMLSelectElement>("#nh-browser");
  nativeHostSelect.addEventListener("change", () => {
    nativeHostBrowser = nativeHostSelect.value as "chrome" | "firefox";
  });
  chordRecorder = requireElement<HTMLButtonElement>("#chord-recorder");
  chordValue = requireElement<HTMLSpanElement>("#chord-recorder-value");
  chordHint = requireElement<HTMLSpanElement>("#chord-recorder-hint");
  chordHiddenInput = requireElement<HTMLInputElement>('#shortcut-form input[name="chord"]');
  chordRecorder.addEventListener("click", (event) => {
    event.preventDefault();
    beginChordCapture();
  });
  chordRecorder.addEventListener("blur", () => endChordCapture());
  requireElement<HTMLFormElement>("#shortcut-form").addEventListener("submit", addShortcut);
  requireElement<HTMLButtonElement>("#nh-register").addEventListener("click", registerNativeHost);
}

function render(): void {
  // Polling must not replace live form controls: focus, selection and IME
  // composition belong to the existing DOM nodes, not a copied draft.

  const connectionStatus = requireElement("#connection-status");
  connectionStatus.textContent = statusMessage;
  connectionStatus.classList.toggle("connected", snapshot.tabs.length > 0);

  requireElement("#shortcut-count").textContent = `${settings.shortcuts.length} 個快捷鍵`;
  const list = requireElement("#shortcut-list");
  if (renderedShortcuts !== settings.shortcuts) {
    list.replaceChildren();
    list.classList.toggle("empty", settings.shortcuts.length === 0);
    if (settings.shortcuts.length === 0) {
      list.textContent = "尚無快捷鍵。新增後即可選取。";
    } else {
      settings.shortcuts.forEach((shortcut) => {
        const label = document.createElement("label");
        label.className = "shortcut-card";
        const input = document.createElement("input");
        input.type = "radio";
        input.name = "shortcut";
        input.value = shortcut.id;
        input.addEventListener("change", () => {
          selectedShortcutId = shortcut.id;
          render();
        });
        const name = document.createElement("strong");
        name.textContent = shortcut.name;
        const chord = document.createElement("code");
        chord.textContent = shortcut.chord;
        label.append(input, name, chord);
        list.append(label);
      });
    }
    renderedShortcuts = settings.shortcuts;
  }
  list.querySelectorAll<HTMLInputElement>("input").forEach((input) => {
    input.checked = input.value === selectedShortcutId;
  });

  populateTargets("#first-target", selectedFirstTarget);
  populateTargets("#second-target", selectedSecondTarget);

  const cooldown = requireElement("#cooldown");
  cooldown.textContent = snapshot.cooldown_remaining_seconds
    ? `冷卻 ${snapshot.cooldown_remaining_seconds} 秒`
    : "可發送";
  const shortcut = selectedShortcut();
  const firstTarget = selectedTab(selectedFirstTarget);
  const secondTarget = selectedTab(selectedSecondTarget);
  requireElement("#proof").textContent = shortcut && firstTarget && secondTarget
    ? `「${shortcut.name}」${shortcut.chord} → ${formatTab(firstTarget)}、${formatTab(secondTarget)}`
    : "選取一個快捷鍵與兩個已連線目標後，才可發送。";
  requireElement("#result").textContent = dispatchResult;
  requireElement("#nh-result").textContent = nativeHostResult;

  requireElement<HTMLButtonElement>("#dispatch").disabled = !canDispatch();
  requireElement<HTMLButtonElement>('#shortcut-form button[type="submit"]').disabled = savingShortcut;
}

function populateTargets(selector: string, selected: string | undefined): void {
  const select = requireElement<HTMLSelectElement>(selector);
  const unchanged = select.options.length === snapshot.tabs.length + 1
    && snapshot.tabs.every((tab, index) => {
      const option = select.options[index + 1];
      return option.value === targetKey(tab) && option.textContent === formatTab(tab);
    });
  if (!unchanged) {
    select.replaceChildren(new Option("重新選擇目標", ""));
    snapshot.tabs.forEach((tab) => {
      select.append(new Option(formatTab(tab), targetKey(tab)));
    });
  }
  select.value = selected ?? "";
  if (select.selectedIndex === -1) select.selectedIndex = 0;
}

async function registerNativeHost(): Promise<void> {
  const browser = nativeHostBrowser;
  nativeHostResult = "正在登錄 native host…";
  render();
  try {
    const result = await invoke<NativeHostRegistrationResult>("register_native_host", {
      request: {
        browser,
        hostPath: null,
      },
    });
    const warning = result.hostExists ? "" : "（注意：native host binary 目前不存在）";
    nativeHostResult = `已寫入 ${result.manifestPath}。${result.registryLocation}${warning}`;
  } catch (error) {
    nativeHostResult = `登錄失敗：${String(error)}`;
  }
  render();
}

async function addShortcut(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  if (savingShortcut) return;
  const form = event.currentTarget as HTMLFormElement;
  const values = new FormData(form);
  try {
    const chord = normalizeChord(String(values.get("chord") ?? ""));
    const name = String(values.get("name") ?? "").trim();
    if (!name) throw new Error("快捷鍵名稱不可為空白。");
    if (!repository) throw new Error("持久化設定尚未可用。");
    const shortcut = { id: crypto.randomUUID(), name, chord, order: settings.shortcuts.length };
    savingShortcut = true;
    render();
    settings = await repository.replaceShortcuts([...settings.shortcuts, shortcut]);
    selectedShortcutId = shortcut.id;
    const currentValues = new FormData(form);
    if (currentValues.get("name") === values.get("name")
      && currentValues.get("chord") === values.get("chord")) {
      form.reset();
      setChord(null);
    }
    statusMessage = `已儲存「${name}」。`;
  } catch (error) {
    statusMessage = error instanceof Error ? error.message : "無法新增快捷鍵。";
  } finally {
    savingShortcut = false;
    render();
  }
}

async function dispatchShortcut(): Promise<void> {
  const shortcut = selectedShortcut();
  const firstTarget = selectedTab(selectedFirstTarget);
  const secondTarget = selectedTab(selectedSecondTarget);
  if (!shortcut || !firstTarget || !secondTarget) return;

  dispatchResult = "正在要求 native host 驗證兩個目標…";
  render();
  try {
    const outcome = await invoke<DispatchOutcome>("request_dispatch", {
      request: {
        request_id: crypto.randomUUID(),
        shortcut: {
          ...shortcut,
          chord: parseChord(shortcut.chord),
        },
        first_target: firstTarget.target,
        second_target: secondTarget.target,
      },
    });
    dispatchResult = formatOutcome(outcome);
    await refreshRuntime();
  } catch (error) {
    dispatchResult = error instanceof Error ? error.message : "發送請求失敗。";
    render();
  }
}

function parseChord(chord: string): { modifiers: string[]; key: Record<string, unknown> } {
  const segments = chord.split("+");
  const main = segments.at(-1)!;
  const modifiers = segments.slice(0, -1).map((modifier) => modifier.toLowerCase());
  if (/^F\d+$/.test(main)) return { modifiers, key: { function: Number(main.slice(1)) } };
  if (main.length === 1) return { modifiers, key: { character: main } };
  return { modifiers, key: { [main === "Esc" ? "escape" : main.toLowerCase()]: null } };
}

function formatOutcome(outcome: DispatchOutcome): string {
  if ("rejected" in outcome) return `已拒絕：${outcome.rejected.reason}`;
  if ("partial" in outcome) return "partial：部分目標已嘗試；請查看各目標結果。";
  return "已嘗試發送；此結果不代表快捷鍵已送達。";
}

const BRIDGE_REJECTION_TEXT: Record<string, string> = {
  rejected_disconnected: "capability token 已失效（通常因為 App 重新啟動；extension 會自動重試）。",
  protocol_mismatch: "協定版本不符（請載入與 App 同版本的 extension）。",
  invalid_message: "native host 送來無法解析的訊息（extension 會自動重試）。",
  unsupported_message: "native host 送來不支援的訊息（請重新載入 extension）。",
};

async function refreshRuntime(): Promise<void> {
  try {
    snapshot = await invoke<RuntimeSnapshot>("runtime_snapshot");
    if (snapshot.tabs.length) {
      statusMessage = `已連線 ${snapshot.tabs.length} 個可選 Browser Tab。`;
    } else if (snapshot.connected_hosts === 0) {
      const rejection = snapshot.last_bridge_rejection
        ? `最近一次 native host 握手被拒絕：${BRIDGE_REJECTION_TEXT[snapshot.last_bridge_rejection] ?? snapshot.last_bridge_rejection}。`
        : "";
      statusMessage = `尚無 native host 連線。請確認 App 已啟動、extension 已載入、且已在此 App 登錄 native host。${rejection}`;
    } else {
      statusMessage = `native host 已連線（${snapshot.connected_hosts} 個 session），但尚未收到 Browser Tab 快照；請重新載入 extension。`;
    }
  } catch {
    statusMessage = "此網頁預覽未連接 Tauri 桌面協調器；啟動 App 後才可讀取 Browser Tab。";
  }
  render();
}

function requireElement<T extends Element = HTMLElement>(selector: string): T {
  const element = app.querySelector<T>(selector);
  if (!element) throw new Error(`缺少必要介面元素：${selector}`);
  return element;
}

async function bootstrap(): Promise<void> {
  try {
    repository = await SettingsRepository.open();
    settings = await repository.read();
    selectedShortcutId = settings.shortcuts.at(0)?.id;
  } catch (error) {
    statusMessage = error instanceof Error ? error.message : "持久化設定不可用。";
  }
  await refreshRuntime();
  window.setInterval(() => void refreshRuntime(), 1_000);
}

mount();
void bootstrap();
