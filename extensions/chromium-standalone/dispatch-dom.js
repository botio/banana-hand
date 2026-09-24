// Serialized by chrome.scripting; keep this function self-contained.
export function dispatchDomChord(chord) {
  if (!document.hasFocus()) return { attempted: false };
  let target = document.activeElement || document.body;
  while (target?.shadowRoot?.activeElement) target = target.shadowRoot.activeElement;
  // Only the deepest focused frame may send: never broadcast a trading hotkey.
  if (!target || /^(IFRAME|FRAME)$/.test(target.tagName)) return { attempted: false };
  const pressed = [];
  let modifiers = 0;
  let attempted = false;
  let error;
  const emit = (type, key) => {
    target.dispatchEvent(new KeyboardEvent(type, {
      key: key.key, code: key.code, location: key.bit ? 1 : 0,
      keyCode: key.windowsVirtualKeyCode, which: key.windowsVirtualKeyCode,
      ctrlKey: !!(modifiers & 2), altKey: !!(modifiers & 1),
      shiftKey: !!(modifiers & 8), metaKey: !!(modifiers & 4),
      bubbles: true, cancelable: true, composed: true, repeat: false,
    }));
  };
  try {
    for (const key of [...chord.modifiers, chord.key]) {
      if (!document.hasFocus() || !target.isConnected) throw new Error('頁面焦點或事件目標已變更');
      if (key.bit) modifiers |= key.bit;
      pressed.push(key);
      attempted = true;
      emit('keydown', key);
    }
  } catch (failure) {
    error = String(failure);
  } finally {
    for (const key of pressed.reverse()) {
      if (key.bit) modifiers &= ~key.bit;
      try { emit('keyup', key); } catch (failure) { error = String(failure); }
    }
  }
  return { attempted, error };
}
