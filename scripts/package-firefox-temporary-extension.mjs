// Packages the Firefox extension source for Firefox's "Load Temporary Add-on"
// flow. Users extract the ZIP, then select its manifest.json from
// about:debugging. Firefox unloads temporary add-ons on restart.
//
// Output: dist/firefox-temporary-extension/banana-hand-firefox-temporary-<version>.zip
import { execFileSync } from "node:child_process";
import { mkdir, rm } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const extDir = join(repoRoot, "extensions", "firefox");

const manifest = JSON.parse(readFileSync(join(extDir, "manifest.json"), "utf8"));
const version = manifest.version ?? "0.0.0";
const archiveRoot = `banana-hand-firefox-temporary-${version}`;
const outDir = join(repoRoot, "dist", "firefox-temporary-extension");
const out = join(outDir, `${archiveRoot}.zip`);

await mkdir(outDir, { recursive: true });
await rm(out, { force: true });

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
execFileSync("python3", ["-c", py, extDir, out, archiveRoot], { stdio: "inherit" });
console.log(`[package-firefox-temporary-extension] ${out}`);
