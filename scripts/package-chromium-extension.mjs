// Official Chromium packages now come from the standalone extension.
// The development ZIP keeps the historical key/ID; the Web Store ZIP omits it.
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, rm } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const source = join(root, 'extensions', 'chromium-standalone');
const legacy = JSON.parse(await readFile(join(root, 'extensions', 'chromium', 'manifest.json'), 'utf8'));
const manifest = JSON.parse(await readFile(join(source, 'manifest.json'), 'utf8'));
const key = legacy.key;
if (!key) throw new Error('missing historical Chromium key');
const directory = join(root, 'dist', 'chromium-extension');
const dev = join(directory, `banana-hand-chromium-${manifest.version}.zip`);
const store = join(directory, `banana-hand-chrome-webstore-${manifest.version}.zip`);
await mkdir(directory, { recursive: true });
await rm(dev, { force: true });
await rm(store, { force: true });
execFileSync('python3', ['-c', `
import json, pathlib, sys, zipfile
source, dev, store, key = sys.argv[1:]
source = pathlib.Path(source)
manifest = json.loads((source / 'manifest.json').read_text())
manifest['key'] = key
files = sorted(path for path in source.rglob('*') if path.is_file() and path.name != 'manifest.json')
with zipfile.ZipFile(dev, 'w', zipfile.ZIP_DEFLATED) as out, zipfile.ZipFile(store, 'w', zipfile.ZIP_DEFLATED) as upload:
    payload = json.dumps(manifest, ensure_ascii=False, indent=2) + '\\n'
    out.writestr('manifest.json', payload)
    upload.writestr('manifest.json', json.dumps({k: v for k, v in manifest.items() if k != 'key'}, ensure_ascii=False, indent=2) + '\\n')
    for path in files:
        out.write(path, path.relative_to(source))
        upload.write(path, path.relative_to(source))
print(dev)
print(store)
`, source, dev, store, key], { stdio: 'inherit' });
