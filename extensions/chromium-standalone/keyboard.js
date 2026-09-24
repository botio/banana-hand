const MODIFIERS = [
  { name: "Ctrl", key: "Control", code: "ControlLeft", windowsVirtualKeyCode: 17, bit: 2 },
  { name: "Alt", key: "Alt", code: "AltLeft", windowsVirtualKeyCode: 18, bit: 1 },
  { name: "Shift", key: "Shift", code: "ShiftLeft", windowsVirtualKeyCode: 16, bit: 8 },
  { name: "Meta", key: "Meta", code: "MetaLeft", windowsVirtualKeyCode: 91, bit: 4 },
];

const MODIFIER_ALIASES = new Map([
  ["ctrl", "Ctrl"], ["control", "Ctrl"],
  ["alt", "Alt"], ["option", "Alt"],
  ["shift", "Shift"],
  ["meta", "Meta"], ["cmd", "Meta"], ["command", "Meta"],
  ["win", "Meta"], ["super", "Meta"],
]);

const NAMED_KEYS = new Map([
  ["esc", { name: "Esc", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 }],
  ["enter", { name: "Enter", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 }],
  ["tab", { name: "Tab", key: "Tab", code: "Tab", windowsVirtualKeyCode: 9 }],
  ["space", { name: "Space", key: " ", code: "Space", windowsVirtualKeyCode: 32 }],
  ["backspace", { name: "Backspace", key: "Backspace", code: "Backspace", windowsVirtualKeyCode: 8 }],
  ["delete", { name: "Delete", key: "Delete", code: "Delete", windowsVirtualKeyCode: 46 }],
  ["insert", { name: "Insert", key: "Insert", code: "Insert", windowsVirtualKeyCode: 45 }],
  ["home", { name: "Home", key: "Home", code: "Home", windowsVirtualKeyCode: 36 }],
  ["end", { name: "End", key: "End", code: "End", windowsVirtualKeyCode: 35 }],
  ["pageup", { name: "PageUp", key: "PageUp", code: "PageUp", windowsVirtualKeyCode: 33 }],
  ["pagedown", { name: "PageDown", key: "PageDown", code: "PageDown", windowsVirtualKeyCode: 34 }],
  ["arrowleft", { name: "ArrowLeft", key: "ArrowLeft", code: "ArrowLeft", windowsVirtualKeyCode: 37 }],
  ["arrowup", { name: "ArrowUp", key: "ArrowUp", code: "ArrowUp", windowsVirtualKeyCode: 38 }],
  ["arrowright", { name: "ArrowRight", key: "ArrowRight", code: "ArrowRight", windowsVirtualKeyCode: 39 }],
  ["arrowdown", { name: "ArrowDown", key: "ArrowDown", code: "ArrowDown", windowsVirtualKeyCode: 40 }],
]);

const KEY_ALIASES = new Map([
  ["escape", "esc"], ["return", "enter"], ["spacebar", "space"],
  ["del", "delete"], ["ins", "insert"], ["pgup", "pageup"], ["pgdn", "pagedown"],
  ["left", "arrowleft"], ["up", "arrowup"], ["right", "arrowright"], ["down", "arrowdown"],
]);

function parseMainKey(value, shifted) {
  if (/^[a-z]$/i.test(value)) {
    const letter = value.toUpperCase();
    return { name: letter, key: shifted ? letter : letter.toLowerCase(), code: `Key${letter}`, windowsVirtualKeyCode: letter.charCodeAt(0) };
  }
  if (/^[0-9]$/.test(value)) {
    return { name: value, key: shifted ? ")!@#$%^&*("[Number(value)] : value, code: `Digit${value}`, windowsVirtualKeyCode: value.charCodeAt(0) };
  }
  if (/^f([1-9]|1[0-9]|2[0-4])$/i.test(value)) {
    const name = value.toUpperCase();
    return { name, key: name, code: name, windowsVirtualKeyCode: 111 + Number(value.slice(1)) };
  }
  const lower = value.toLowerCase();
  const named = NAMED_KEYS.get(KEY_ALIASES.get(lower) ?? lower);
  if (named) return { ...named };
  throw new Error(`不支援的主要按鍵：${value}。可用 A–Z、0–9、F1–F24、Esc、Enter、Tab、Space、Backspace、Delete、Insert、Home、End、PageUp、PageDown 與方向鍵。`);
}

export function parseChord(raw) {
  if (typeof raw !== "string" || !raw.trim()) throw new Error("快捷鍵不可為空白。");
  if (raw.length > 120) throw new Error("快捷鍵過長；只接受一組組合鍵，不接受連續按鍵或巨集。");
  const segments = raw.split("+").map((segment) => segment.trim());
  if (segments.some((segment) => !segment)) throw new Error("快捷鍵不可包含空白片段。");
  const selected = new Set();
  let main;
  for (const segment of segments) {
    const modifier = MODIFIER_ALIASES.get(segment.toLowerCase());
    if (modifier) {
      if (main !== undefined) throw new Error("修飾鍵必須在主要按鍵前。");
      if (selected.has(modifier)) throw new Error(`修飾鍵不可重複：${modifier}。`);
      selected.add(modifier);
    } else {
      if (main !== undefined) throw new Error("每組快捷鍵只能有一個主要按鍵；不接受連續按鍵或巨集。");
      main = segment;
    }
  }
  if (main === undefined) throw new Error("快捷鍵必須包含主要按鍵，不能只有修飾鍵。");
  const modifiers = MODIFIERS.filter((modifier) => selected.has(modifier.name));
  const key = parseMainKey(main, selected.has("Shift"));
  return {
    chord: [...modifiers.map((modifier) => modifier.name), key.name].join("+"),
    modifiers,
    key,
  };
}

export function normalizeChord(raw) {
  return parseChord(raw).chord;
}

