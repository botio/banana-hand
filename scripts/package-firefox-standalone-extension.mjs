import { execFileSync } from 'node:child_process';
import { cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const source = join(root, 'extensions', 'chromium-standalone');
const manifest = JSON.parse(await readFile(join(source, 'manifest.json'), 'utf8'));
const directory = join(root, 'dist', 'firefox-standalone');
const stage = join(directory, 'extension');
await mkdir(directory, { recursive: true });
await rm(stage, { recursive: true, force: true });
await cp(source, stage, { recursive: true });
delete manifest.minimum_chrome_version;
manifest.background = { page: 'background.html' };
manifest.browser_specific_settings = {
  gecko: {
    id: 'standalone@banana-hand.dev',
    strict_min_version: '140.0',
    data_collection_permissions: { required: ['none'] },
  },
  gecko_android: { strict_min_version: '142.0' },
};
await writeFile(join(stage, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
await writeFile(join(stage, 'background.html'), '<!doctype html><meta charset="utf-8"><script type="module" src="background.js"></script>\n');
const output = join(directory, `banana-hand-firefox-standalone-temporary-${manifest.version}.zip`);
await rm(output, { force: true });
execFileSync('python3', ['-c', `
import pathlib, sys, zipfile
source, output = map(pathlib.Path, sys.argv[1:])
with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(source.rglob('*')):
        if path.is_file():
            archive.write(path, path.relative_to(source))
print(output)
`, stage, output], { stdio: 'inherit' });
