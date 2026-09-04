import { mkdir, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, resolve } from "node:path";

const options = Object.fromEntries(
  process.argv.slice(2).map((argument) => {
    const [key, value] = argument.split("=", 2);
    return [key.replace(/^--/, ""), value];
  }),
);
const browser = options.browser;
const hostPath = options["host-path"];
const output = options.out;
const extensionId = options["extension-id"];

if (!browser || !hostPath || !output || !extensionId) {
  throw new Error("用法：--browser=chrome|firefox --host-path=/absolute/host --extension-id=id --out=/absolute/manifest.json");
}
if (!["chrome", "firefox"].includes(browser)) {
  throw new Error(`不支援的 browser：${browser}`);
}
if (!isAbsolute(hostPath) || !isAbsolute(output)) {
  throw new Error("host-path 與 out 必須是絕對路徑。");
}

const manifest = {
  name: "dev.bananahand.dispatch_host",
  description: "Banana Hand Native Messaging host",
  path: resolve(hostPath),
  type: "stdio",
  ...(browser === "firefox"
    ? { allowed_extensions: [extensionId] }
    : { allowed_origins: [`chrome-extension://${extensionId}/`] }),
};

await mkdir(dirname(output), { recursive: true });
await writeFile(output, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o600 });
process.stdout.write(`${output}\n`);
