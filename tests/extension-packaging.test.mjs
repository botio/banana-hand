import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const repo = new URL("../", import.meta.url);

test("Chromium packaging separates keyless store uploads from fixed-ID development installs", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "banana-extension-package-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, "scripts"));
  await cp(new URL("scripts/package-chromium-extension.mjs", repo), join(root, "scripts/package-chromium-extension.mjs"));
  await cp(new URL("extensions/chromium/", repo), join(root, "extensions/chromium"), { recursive: true });
  const manifestPath = join(root, "extensions/chromium/manifest.json");
  const originalManifest = await readFile(manifestPath);
  execFileSync(process.execPath, [join(root, "scripts/package-chromium-extension.mjs")]);
  assert.deepEqual(await readFile(manifestPath), originalManifest, "packaging must not modify the source manifest");
  execFileSync("python3", ["-c", `
import json, pathlib, sys, zipfile
root = pathlib.Path(sys.argv[1])
source = root / "extensions/chromium"
original = (source / "manifest.json").read_bytes()
manifest = json.loads(original)
version = manifest["version"]
out = root / "dist/chromium-extension"
with zipfile.ZipFile(out / f"banana-hand-chromium-{version}.zip") as dev, zipfile.ZipFile(out / f"banana-hand-chrome-webstore-{version}.zip") as store:
    prefix = f"banana-hand-chromium-{version}/"
    assert dev.read(prefix + "manifest.json") == original, "development ID must not change"
    uploaded = json.loads(store.read("manifest.json"))
    assert "key" not in uploaded, "Chrome Web Store rejects the development key"
    assert uploaded == {key: value for key, value in manifest.items() if key != "key"}
    files = {path.relative_to(source).as_posix(): path for path in source.rglob("*") if path.is_file()}
    assert set(store.namelist()) == set(files), "store manifest and assets must be at the archive root"
    assert set(dev.namelist()) == {prefix + name for name in files}
    for name, path in files.items():
        assert dev.read(prefix + name) == path.read_bytes()
        if name != "manifest.json":
            assert store.read(name) == path.read_bytes(), f"store asset changed: {name}"
`, root]);
});
