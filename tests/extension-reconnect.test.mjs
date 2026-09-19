import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { webcrypto } from "node:crypto";
import { setImmediate } from "node:timers/promises";
import vm from "node:vm";
import test from "node:test";

function event() {
  const listeners = [];
  return {
    addListener(listener) { listeners.push(listener); },
    emit(value) { for (const listener of listeners) listener(value); },
  };
}

async function extension(kind) {
  const ports = [];
  const timers = new Map();
  const alarms = new Map();
  let nextTimer = 1;
  // Controllable tab/window focus so prepare tests can model the OS-focus
  // commit lagging the API promise (the Windows second-round race).
  const focus = { active: false, focused: false, autocommit: true };
  const api = {
    tabs: {
      async get(tabId) {
        return { id: tabId, windowId: 1, active: focus.active };
      },
      async update(tabId, props) {
        if (Object.hasOwn(props, "active")) focus.active = props.active;
        return api.tabs.get(tabId);
      },
      ...Object.fromEntries(["onCreated", "onRemoved", "onReplaced", "onUpdated", "onAttached", "onDetached"].map(name => [name, event()])),
    },
    windows: {
      async getAll() { return []; },
      async get(windowId) {
        return { id: windowId, focused: focus.focused };
      },
      async update(windowId, props) {
        // The API promise resolves when the focus request is accepted; the OS
        // activation commits asynchronously (a fresh timer). Turn autocommit
        // off to model a focus that never lands (the fail-closed path).
        if (Object.hasOwn(props, "focused") && props.focused && focus.autocommit) {
          setTimeout(() => { focus.focused = true; }, 0);
        }
        return api.windows.get(windowId);
      },
      onRemoved: event(),
    },
    runtime: {
      onStartup: event(),
      connectNative() {
        const port = {
          onMessage: event(), onDisconnect: event(), messages: [], closed: false,
          postMessage(message) {
            assert.ok(!this.closed, "posting to a closed native port");
            this.messages.push(message);
          },
          // Per runtime.Port: local disconnect does NOT fire local onDisconnect.
          disconnect() { this.closed = true; },
        };
        ports.push(port);
        return port;
      },
    },
    storage: { local: { async get() { return { browserInstanceId: "profile" }; } } },
    alarms: { create(name, info) { alarms.set(name, info); }, onAlarm: event() },
  };
  const source = await readFile(new URL(`../extensions/${kind}/background.js`, import.meta.url), "utf8");
  vm.runInNewContext(source, {
    [kind === "chromium" ? "chrome" : "browser"]: api,
    crypto: webcrypto,
    console: { warn() {} },
    setTimeout(callback) { const id = nextTimer++; timers.set(id, callback); return id; },
    clearTimeout(id) { timers.delete(id); },
  });
  await setImmediate();
  return {
    api, ports, timers, alarms, focus,
    async retry() {
      const callbacks = [...timers.values()];
      timers.clear();
      for (const callback of callbacks) callback();
      await setImmediate();
    },
  };
}

// Build a valid prepare message for a harness whose hello we have already
// read, so prepareTarget's session validation passes.
function prepareMessage(hello, windowId = 1, tabId = 7) {
  return {
    type: "prepare",
    request_id: "req-1",
    target: {
      browser: hello.browser,
      browser_instance_id: hello.browser_instance_id,
      session_nonce: hello.session_nonce,
      window_id: windowId,
      tab_id: tabId,
      generation: 0,
    },
  };
}

for (const kind of ["chromium", "firefox"]) {
  test(`${kind}: rejected handshake closes its port and reconnects without a remote disconnect event`, async () => {
    const harness = await extension(kind);
    const first = harness.ports[0];
    first.onMessage.emit({ type: "error", code: "rejected_disconnected" });
    await harness.retry();
    assert.ok(first.closed);
    const hello = harness.ports[1]?.messages.find(message => message.type === "hello");
    assert.equal(hello?.browser_instance_id, "profile", "a new native host must receive hello after local disconnect");
  });

  test(`${kind}: prepare withholds ready:true until the target tab is active in a focused window`, async () => {
    const harness = await extension(kind);
    const hello = harness.ports[0].messages.find(message => message.type === "hello");
    // Model the Windows second-round race: tab activation is requested but the
    // OS focus commit lags the API promise. Keep `focused` false, prove no
    // premature ready, then flip focus to model the commit arriving.
    harness.focus.autocommit = false;
    harness.ports[0].onMessage.emit(prepareMessage(hello));
    await setImmediate();
    const preparedBefore = harness.ports[0].messages.filter(message => message.type === "prepared");
    assert.equal(preparedBefore.length, 0, "must not report ready before focus actually lands");

    harness.focus.focused = true; // the OS focus commit finally lands
    let prepared = [];
    for (let i = 0; i < 6 && prepared.length === 0; i += 1) {
      await harness.retry();
      prepared = harness.ports[0].messages.filter(message => message.type === "prepared");
    }
    assert.equal(prepared.length, 1, "exactly one prepared result after focus commits");
    assert.equal(prepared[0].ready, true, "ready only after the target owns focus");
    assert.equal(prepared[0].code, undefined, "no failure code on the happy path");
  });

  test(`${kind}: prepare reports focus_failed and never ready:true when the target never gains focus`, async () => {
    const harness = await extension(kind);
    const hello = harness.ports[0].messages.find(message => message.type === "hello");
    harness.focus.autocommit = false; // the window never becomes the focus target
    harness.ports[0].onMessage.emit(prepareMessage(hello));
    for (let i = 0; i < 24; i += 1) await harness.retry();
    const prepared = harness.ports[0].messages.filter(message => message.type === "prepared");
    assert.equal(prepared.length, 1, "exactly one prepared result when focus never lands");
    assert.equal(prepared[0].ready, false, "fail-closed: not ready");
    assert.equal(prepared[0].code, "focus_failed", "identify the gate that refused");
    assert.ok(!prepared.some(message => message.ready === true), "never fabricate a ready acknowledgment");
  });

  test(`${kind}: stales target reports rejected_stale before any activation`, async () => {
    const harness = await extension(kind);
    const hello = harness.ports[0].messages.find(message => message.type === "hello");
    harness.ports[0].onMessage.emit(prepareMessage({ ...hello, session_nonce: "other-nonce" }));
    await setImmediate();
    const prepared = harness.ports[0].messages.filter(message => message.type === "prepared");
    assert.equal(prepared.length, 1);
    assert.equal(prepared[0].ready, false);
    assert.equal(prepared[0].code, "rejected_stale");
    assert.ok(harness.ports[0].messages.filter(message => message.type === "prepared" && message.ready === true).length === 0);
  });
}

test("Firefox: alarm reconnects after an idle event page loses its retry timer", async () => {
  const harness = await extension("firefox");
  harness.ports[0].closed = true;
  harness.ports[0].onDisconnect.emit(harness.ports[0]);
  harness.timers.clear();
  for (const name of harness.alarms.keys()) harness.api.alarms.onAlarm.emit({ name });
  await setImmediate();
  assert.equal(harness.ports[1]?.messages[0]?.type, "hello", "browser alarm must restore the connection without Tab activity");
});

test("Firefox: next hello preserves the native port error", async () => {
  const harness = await extension("firefox");
  const port = harness.ports[0];
  port.error = { message: "Native application disconnected" };
  port.closed = true;
  port.onDisconnect.emit(port);
  await harness.retry();
  const hello = harness.ports[1]?.messages.find(message => message.type === "hello");
  assert.equal(hello?.last_disconnect_reason, port.error.message);
});
