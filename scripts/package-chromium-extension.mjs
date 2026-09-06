// Produces a fixed-ID development ZIP and a separate Chrome Web Store upload ZIP.
// The store manifest must be at the archive root and omit the development key.
//
// Outputs: dist/chromium-extension/banana-hand-{chromium,chrome-webstore}-<version>.zip
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
const storeOut = join(outDir, `banana-hand-chrome-webstore-${version}.zip`);

await mkdir(outDir, { recursive: true });
await rm(out, { force: true });
await rm(storeOut, { force: true });

// Keep the development archive unchanged; transform only the store manifest.
const py = `
import json, os, sys, zipfile
src, out, store_out, root = sys.argv[1:]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zf, zipfile.ZipFile(store_out, "w", zipfile.ZIP_DEFLATED) as store:
    for current, _, files in os.walk(src):
        for filename in sorted(files):
            full = os.path.join(current, filename)
            relative = os.path.relpath(full, src)
            zf.write(full, os.path.join(root, relative))
            if relative == "manifest.json":
                with open(full, encoding="utf-8") as manifest_file:
                    manifest = json.load(manifest_file)
                manifest.pop("key", None)
                store.writestr(relative, json.dumps(manifest, ensure_ascii=False, indent=2) + "\\n")
            else:
                store.write(full, relative)
print("packed", out)
print("packed", store_out)
`;
execFileSync("python3", ["-c", py, extDir, out, storeOut, `banana-hand-chromium-${version}`], { stdio: "inherit" });
console.log(`[package-chromium-extension] ${out}`);
console.log(`[package-chromium-extension] ${storeOut}`);
