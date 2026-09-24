import { normalizeChord } from './keyboard.js';

const chrome = globalThis.browser ?? globalThis.chrome;

const byId = (id) => document.getElementById(id);
const controls = {
  targets: [byId('target-one'), byId('target-two')],
  refresh: byId('refresh-tabs'),
  saveTargets: byId('save-targets'),
  shortcuts: byId('shortcut-list'),
  form: byId('shortcut-form'),
  name: byId('shortcut-name'),
  chord: byId('shortcut-chord'),
  capture: byId('capture-key'),
  saveShortcut: byId('save-shortcut'),
  newShortcut: byId('new-shortcut'),
  cancelEdit: byId('cancel-edit'),
  selected: byId('dispatch-shortcut'),
  send: byId('send'),
};
let state = null;
let connected = false;
let pending = false;
let needsStateConfirmation = false;
let polling = false;
let generation = 0;
let targetDraft = [null, null];
let targetsDirty = false;
let editingId = null;
let capturing = false;
let targetsFingerprint = '';
let shortcutsFingerprint = '';
let resultFingerprint = '';

function message(text = '') {
  byId('message').textContent = text;
  byId('message').hidden = !text;
}

async function request(payload) {
  let response;
  try {
    response = await chrome.runtime.sendMessage(payload);
    if (!response || typeof response.ok !== 'boolean') {
      throw new Error('擴充功能未回傳有效回應。');
    }
  } catch (error) {
    connected = false;
    throw new Error(`無法連線至擴充功能：${error.message || String(error)}`);
  }
  connected = true;
  if (!response.ok) throw new Error(response.error || '操作未完成。');
  return response;
}

function option(value, text) {
  const element = document.createElement('option');
  element.value = value;
  element.textContent = text;
  return element;
}

function tabDescription(tab) {
  return `${tab.title || '未命名分頁'} — ${tab.url || '網址不可讀取'} [#${tab.id}]`;
}

function renderTargets() {
  if (!state) return;
  const fingerprint = JSON.stringify([state.tabs, targetDraft]);
  if (fingerprint === targetsFingerprint) return;
  // A background refresh must not close a selector while the user is choosing.
  if (controls.targets.includes(document.activeElement)) return;
  targetsFingerprint = fingerprint;
  controls.targets.forEach((select, index) => {
    const draft = targetDraft[index];
    const options = [option('', '選擇分頁')];
    for (const tab of state.tabs) options.push(option(String(tab.id), tabDescription(tab)));
    if (draft !== null && !state.tabs.some((tab) => tab.id === draft)) {
      options.push(option(String(draft), `分頁 #${draft} 已關閉或無法使用，請重新選擇`));
    }
    select.replaceChildren(...options);
    select.value = draft === null ? '' : String(draft);
  });
}

function resetEditor() {
  editingId = null;
  controls.form.reset();
  stopCapture();
  byId('editor-title').textContent = '新增快捷鍵';
  controls.saveShortcut.textContent = '儲存快捷鍵';
  controls.cancelEdit.hidden = true;
}

function editShortcut(shortcut) {
  editingId = shortcut.id;
  controls.name.value = shortcut.name;
  controls.chord.value = shortcut.chord;
  stopCapture();
  byId('editor-title').textContent = '編輯快捷鍵';
  controls.saveShortcut.textContent = '儲存變更';
  controls.cancelEdit.hidden = false;
  controls.name.focus();
}

function renderShortcuts() {
  if (!state) return;
  const fingerprint = JSON.stringify(state.shortcuts);
  if (fingerprint === shortcutsFingerprint) return;
  shortcutsFingerprint = fingerprint;
  const selected = controls.selected.value;
  controls.selected.replaceChildren(option('', '選擇快捷鍵'));
  controls.shortcuts.replaceChildren();
  for (const shortcut of state.shortcuts) {
    controls.selected.append(option(shortcut.id, `${shortcut.name} · ${shortcut.chord}`));
    const item = document.createElement('li');
    item.className = 'shortcut-item';
    const info = document.createElement('div');
    info.className = 'shortcut-info';
    const name = document.createElement('strong');
    name.textContent = shortcut.name;
    const chord = document.createElement('kbd');
    chord.textContent = shortcut.chord;
    info.append(name, chord);
    const actions = document.createElement('div');
    actions.className = 'shortcut-actions';
    const edit = document.createElement('button');
    edit.type = 'button';
    edit.className = 'button subtle';
    edit.textContent = '編輯';
    edit.setAttribute('aria-label', `編輯 ${shortcut.name}`);
    edit.addEventListener('click', () => editShortcut(shortcut));
    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'button subtle danger';
    remove.textContent = '刪除';
    remove.setAttribute('aria-label', `刪除 ${shortcut.name}`);
    remove.addEventListener('click', async () => {
      if (!window.confirm(`確定刪除「${shortcut.name}」？`)) return;
      await configure({ shortcuts: state.shortcuts.filter((entry) => entry.id !== shortcut.id) }, () => {
        if (editingId === shortcut.id) resetEditor();
      });
    });
    actions.append(edit, remove);
    item.append(info, actions);
    controls.shortcuts.append(item);
  }
  if (state.shortcuts.some((shortcut) => shortcut.id === selected)) controls.selected.value = selected;
  if (!state.shortcuts.length) {
    const empty = document.createElement('li');
    empty.className = 'empty-state';
    empty.textContent = '尚無快捷鍵。請在下方新增名稱與按鍵組合。';
    controls.shortcuts.append(empty);
  }
}

function renderResult(result) {
  const fingerprint = JSON.stringify(result);
  if (!result || fingerprint === resultFingerprint) return;
  resultFingerprint = fingerprint;
  const container = byId('results');
  container.replaceChildren();
  const detail = document.createElement('p');
  detail.className = 'result-detail';
  detail.textContent = result.detail || '請至兩個目標分頁核對實際狀態。';
  container.append(detail);
  const labels = { attempted: '已嘗試傳送', failed: '傳送失敗', not_attempted: '未嘗試' };
  for (const target of result.targets || []) {
    const row = document.createElement('div');
    row.className = 'result-row';
    const heading = document.createElement('div');
    heading.className = 'result-heading';
    const name = document.createElement('strong');
    name.textContent = `分頁 #${target.tabId}`;
    const status = document.createElement('span');
    status.className = `result-status ${Object.hasOwn(labels, target.status) ? target.status : 'failed'}`;
    status.textContent = labels[target.status] || '狀態不明';
    heading.append(name, status);
    const text = document.createElement('p');
    text.textContent = target.detail || '請手動確認目標分頁。';
    row.append(heading, text);
    container.append(row);
  }
}

function blockReason() {
  if (!connected || !state) return '尚未連線，請重新整理後再操作。';
  if (pending || state.busy) return '操作處理中，請等待結果；不要重複操作。';
  if (needsStateConfirmation) return '正在重新確認傳送狀態，請勿重複操作。';
  if (state.cooldownUntil > Date.now()) return '冷卻中。請先核對兩邊狀態，不要重複操作。';
  if (targetsDirty) return '目的地尚未儲存，請先按「儲存目的地」。';
  if (targetDraft.some((id) => id === null)) return '請選擇並儲存兩個目的地。';
  if (targetDraft[0] === targetDraft[1]) return '兩個目的地必須是不同分頁。';
  if (targetDraft.some((id) => !state.tabs.some((tab) => tab.id === id))) return '目的地已關閉或無法使用，請重新選擇。';
  const shortcut = state.shortcuts.find((entry) => entry.id === controls.selected.value);
  if (!shortcut) return '請選擇這次要傳送的快捷鍵。';
  try { normalizeChord(shortcut.chord); } catch { return '此快捷鍵格式無效，請先編輯修正。'; }
  return '';
}

function renderControls() {
  const locked = !connected || !state || pending || Boolean(state.busy);
  byId('connection').textContent = connected ? '擴充功能已連線' : '擴充功能未連線';
  byId('connection').classList.toggle('online', connected);
  controls.saveTargets.disabled = locked || !targetsDirty || (targetDraft[0] !== null && targetDraft[0] === targetDraft[1]);
  controls.saveShortcut.disabled = locked;
  controls.newShortcut.disabled = locked;
  controls.name.disabled = pending;
  controls.chord.disabled = pending;
  controls.capture.disabled = pending;
  controls.cancelEdit.disabled = pending;
  controls.selected.disabled = locked;
  for (const button of controls.shortcuts.querySelectorAll('button')) button.disabled = locked;
  for (const select of controls.targets) select.disabled = locked;
  byId('target-status').textContent = targetsDirty ? '有未儲存的變更' : targetDraft.every((id) => id !== null) ? '目的地已儲存' : '尚未完成目的地設定';
  const seconds = Math.max(0, Math.ceil(((state?.cooldownUntil || 0) - Date.now()) / 1000));
  byId('cooldown').textContent = seconds ? `剩餘 ${seconds} 秒` : '15 秒冷卻保護';
  byId('cooldown').classList.toggle('active', seconds > 0);
  byId('dispatch-status').textContent = pending || state?.busy ? '處理中' : seconds ? '冷卻中' : '等待手動傳送';
  controls.send.textContent = pending || state?.busy ? '處理中，請勿重複操作' : seconds ? `冷卻中 · ${seconds} 秒` : '傳送至兩個分頁';
  const reason = blockReason();
  controls.send.disabled = Boolean(reason);
  byId('send-help').textContent = reason || '請再次核對下方分頁的帳戶、商品、數量與輸入焦點，再傳送。';
  targetDraft.forEach((id, index) => {
    const slot = index === 0 ? 'one' : 'two';
    const tab = state?.tabs.find((entry) => entry.id === id);
    byId(`summary-${slot}`).textContent = tab?.title || (id === null ? '尚未選擇' : `分頁 #${id} 無法使用`);
    byId(`summary-${slot}-url`).textContent = tab ? `${tab.url || '網址不可讀取'} · #${id}` : '';
  });
  const shortcut = state?.shortcuts.find((entry) => entry.id === controls.selected.value);
  byId('summary-name').textContent = shortcut?.name || '尚未選擇快捷鍵';
  byId('summary-chord').textContent = shortcut?.chord || '—';
}

function applyState(next) {
  state = next;
  needsStateConfirmation = false;
  if (!targetsDirty) targetDraft = [...next.targets];
  renderTargets();
  renderShortcuts();
  renderResult(next.lastResult);
  renderControls();
}

async function refresh(explicit = false) {
  if (polling) return;
  polling = true;
  controls.refresh.disabled = true;
  const started = generation;
  try {
    const next = await request({ type: 'state' });
    if (started === generation) applyState(next);
    if (explicit) message();
  } catch (error) {
    if (explicit || !state) message(error.message);
  } finally {
    polling = false;
    controls.refresh.disabled = false;
    renderControls();
  }
}

async function configure(update, onSaved = () => {}) {
  if (pending || state?.busy) return;
  pending = true;
  generation += 1;
  message();
  renderControls();
  try {
    const next = await request({ type: 'configure', ...update });
    onSaved(next);
    applyState(next);
  } catch (error) {
    message(error.message);
  } finally {
    pending = false;
    renderControls();
  }
}

function stopCapture() {
  capturing = false;
  controls.chord.readOnly = false;
  controls.capture.textContent = '擷取按鍵';
  controls.capture.setAttribute('aria-pressed', 'false');
  byId('capture-help').textContent = '可直接輸入，或按「擷取按鍵」後按下組合鍵。部分瀏覽器／系統保留鍵無法攔截。';
}

controls.capture.addEventListener('pointerdown', (event) => {
  if (capturing) event.preventDefault();
});
controls.capture.addEventListener('click', () => {
  if (capturing) { stopCapture(); return; }
  capturing = true;
  controls.chord.readOnly = true;
  controls.capture.textContent = '取消擷取';
  controls.capture.setAttribute('aria-pressed', 'true');
  byId('capture-help').textContent = '請按下組合鍵；Esc 取消。若瀏覽器攔截，請改用手動輸入。';
  controls.chord.focus();
});
controls.chord.addEventListener('keydown', (event) => {
  if (!capturing) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.key === 'Escape') { stopCapture(); return; }
  if (event.repeat || ['Control', 'Alt', 'Shift', 'Meta', 'AltGraph'].includes(event.key)) return;
  const main = /^Key[A-Z]$/.test(event.code) ? event.code.slice(3)
    : /^Digit[0-9]$/.test(event.code) ? event.code.slice(5)
      : event.code === 'Space' ? 'Space' : event.key;
  const parts = [];
  if (event.ctrlKey) parts.push('Ctrl');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  if (event.metaKey) parts.push('Meta');
  parts.push(main);
  try {
    controls.chord.value = normalizeChord(parts.join('+'));
    stopCapture();
    // Blur prevents the captured key's remaining keyup from editing the input.
    controls.chord.blur();
  } catch (error) {
    byId('capture-help').textContent = `${error.message} 請改按其他組合鍵，或按 Esc 取消。`;
  }
});
controls.chord.addEventListener('blur', stopCapture);
controls.targets.forEach((select, index) => {
  select.addEventListener('change', () => {
    targetDraft[index] = select.value === '' ? null : Number(select.value);
    targetsDirty = !state || targetDraft.some((id, i) => id !== state.targets[i]);
    renderControls();
  });
  select.addEventListener('blur', renderTargets);
});
controls.saveTargets.addEventListener('click', () => configure({ targets: [...targetDraft] }, () => { targetsDirty = false; }));
controls.refresh.addEventListener('click', () => refresh(true));
controls.selected.addEventListener('change', renderControls);
controls.newShortcut.addEventListener('click', () => { resetEditor(); controls.name.focus(); });
controls.cancelEdit.addEventListener('click', resetEditor);
controls.form.addEventListener('submit', async (event) => {
  event.preventDefault();
  if (!state || !connected || pending || state.busy) return;
  const name = controls.name.value.trim();
  if (!name) { message('請輸入快捷鍵名稱。'); controls.name.focus(); return; }
  let chord;
  try { chord = normalizeChord(controls.chord.value); } catch (error) { message(error.message); controls.chord.focus(); return; }
  if (editingId && !state.shortcuts.some((shortcut) => shortcut.id === editingId)) {
    message('此快捷鍵已被其他控制頁刪除。請按「新增快捷鍵」重新建立。');
    return;
  }
  const id = editingId || crypto.randomUUID();
  const shortcut = { id, name, chord };
  const shortcuts = editingId ? state.shortcuts.map((entry) => entry.id === editingId ? shortcut : entry) : [...state.shortcuts, shortcut];
  await configure({ shortcuts }, () => resetEditor());
});
function originPattern(url) {
  const parsed = new URL(url);
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') throw new Error('只支援 http 或 https 分頁。');
  return `${parsed.origin}/*`;
}

controls.send.addEventListener('click', async () => {
  if (blockReason()) return;
  const shortcutId = controls.selected.value;
  const patterns = [...new Set(targetDraft.map((id) => {
    const tab = state.tabs.find((entry) => entry.id === id);
    if (!tab?.url) throw new Error('無法讀取目標網址，請重新整理分頁清單。');
    return originPattern(tab.url);
  }))];
  if (!await chrome.permissions.request({ origins: patterns })) {
    message('未允許這兩個分頁所在網站，本次未傳送。不需要預先設定網站清單。');
    return;
  }
  pending = true;
  generation += 1;
  message();
  renderControls();
  try {
    const response = await request({ type: 'dispatch', shortcutId });
    renderResult(response.result);
  } catch (error) {
    message(`${error.message} 若按鍵可能已開始傳送，結果可能不明；請親自核對兩邊狀態。本頁不會自動重試。`);
  } finally {
    // Keep the send control locked until authoritative cooldown/busy state arrives.
    generation += 1;
    needsStateConfirmation = true;
    try {
      applyState(await request({ type: 'state' }));
    } catch (error) {
      message(`${error.message} 傳送狀態尚未確認；請手動核對兩邊分頁，不要重複操作。`);
    }
    pending = false;
    renderControls();
  }
});

renderControls();
void refresh();
setInterval(() => { renderControls(); void refresh(); }, 1000);
