// Builds the native messaging host for a target triple and places it where the
// Tauri bundler expects a sidecar: `src-tauri/binaries/<name>-<target-triple>[.exe]`.
//
// Usage:
//   node scripts/bundle-native-host.mjs                 # host target (release)
import { chmod, cp, mkdir, rm } from "node:fs/promises";
import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const sidecarName = "banana-hand-native-host";

function targetTriple(args) {
  const index = args.indexOf("--target");
  if (index !== -1 && args[index + 1]) return args[index + 1];
  return execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" }).trim();
}

const target = targetTriple(process.argv.slice(2));
const isWindows = target.includes("-windows");
const extension = isWindows ? ".exe" : "";

const source = join(repoRoot, "target", target, "release", sidecarName + extension);
const destinationDir = join(repoRoot, "src-tauri", "binaries");
const destination = join(destinationDir, `${sidecarName}-${target}${extension}`);

console.log(`[bundle-native-host] target ${target}`);
console.log("[bundle-native-host] cargo build -p banana-hand-native-host --release");
execFileSync(
  "cargo",
  ["build", "-p", "banana-hand-native-host", "--release", "--target", target],
  { cwd: repoRoot, stdio: "inherit" },
);

if (!existsSync(source)) {
  throw new Error(`built sidecar not found at ${source}`);
}
await mkdir(destinationDir, { recursive: true });
await rm(destination, { force: true });
await cp(source, destination);
await chmod(destination, 0o755);
console.log(`[bundle-native-host] sidecar ready: ${destination}`);
