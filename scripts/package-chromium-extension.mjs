// Packages the Chromium extension source into a release asset. Users extract the
// `.zip`, then select the extracted folder with Chrome's "Load unpacked" UI.
//
// Output: dist/chromium-extension/banana-hand-chromium-<version>.zip
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdir, rm } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const extDir = join(repoRoot, "extensions", "chromium");

const manifest = JSON.parse(readFileSync(join(extDir, "manifest.json"), "utf8"));
const version = manifest.version ?? "0.0.0";
const fixedExtensionId = "mooakjhlbkjfbmbmliklkmfmacnomlai";
const derivedExtensionId = [...createHash("sha256").update(Buffer.from(manifest.key, "base64")).digest().subarray(0, 16)]
  .map((byte) => String.fromCharCode(97 + (byte >> 4), 97 + (byte & 0x0f)))
  .join("");
if (derivedExtensionId !== fixedExtensionId) {
  throw new Error(`Chromium manifest key resolves to ${derivedExtensionId}, expected ${fixedExtensionId}.`);
}
const outDir = join(repoRoot, "dist", "chromium-extension");
const out = join(outDir, `banana-hand-chromium-${version}.zip`);

await mkdir(outDir, { recursive: true });
await rm(out, { force: true });

// The archive root is the extension directory itself, so extracting it creates
// a folder immediately usable by Chrome's "Load unpacked" flow.
const py = `
import os, sys, zipfile
src, out, root = sys.argv[1], sys.argv[2], sys.argv[3]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zf:
    for current, _, files in os.walk(src):
        for filename in sorted(files):
            full = os.path.join(current, filename)
            relative = os.path.relpath(full, src)
            zf.write(full, os.path.join(root, relative))
print("packed", out)
`;
execFileSync("python3", ["-c", py, extDir, out, `banana-hand-chromium-${version}`], { stdio: "inherit" });
console.log(`[package-chromium-extension] ${out}`);
