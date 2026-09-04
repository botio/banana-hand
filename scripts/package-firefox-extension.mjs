// Packages the Firefox extension (extensions/firefox/) into a distributable
// `.xpi` — a zip archive whose root contains manifest.json (plus background.js
// and any icons/). Firefox's "Install Add-on From File" only accepts a .xpi,
// never a bare manifest.json.
//
// Output: dist/firefox-extension/banana-hand-firefox-<version>.xpi
// (dist/ is gitignored — run this to (re)produce the package after edits.)
//
// NOTE: the produced .xpi is UNSIGNED. Unsigned .xpi can only be installed in
// Firefox Developer Edition / Nightly / ESR with `xpinstall.signatures.required
// = false`; release-channel Firefox requires AMO signing.
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
const outDir = join(repoRoot, "dist", "firefox-extension");
const out = join(outDir, `banana-hand-firefox-${version}.xpi`);

await mkdir(outDir, { recursive: true });
await rm(out, { force: true });

// .xpi = zip with the extension root as the archive root. Use python3's stdlib
// zipfile (no extra npm dependency, deterministic ordering).
const py = `
import os, sys, zipfile
src, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zf:
    for root, _, files in os.walk(src):
        for f in sorted(files):
            full = os.path.join(root, f)
            zf.write(full, os.path.relpath(full, src))
print("packed", out)
`;
execFileSync("python3", ["-c", py, extDir, out], { stdio: "inherit" });
console.log(`[package-firefox-extension] ${out}`);
