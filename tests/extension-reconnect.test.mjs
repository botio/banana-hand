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
  const api = {
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
    windows: { async getAll() { return []; }, onRemoved: event() },
    tabs: Object.fromEntries(["onCreated", "onRemoved", "onReplaced", "onUpdated", "onAttached", "onDetached"].map(name => [name, event()])),
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
    api, ports, timers, alarms,
    async retry() {
      const callbacks = [...timers.values()];
      timers.clear();
      for (const callback of callbacks) callback();
      await setImmediate();
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
