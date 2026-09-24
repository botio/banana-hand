import { execFileSync } from 'node:child_process';
import { mkdir, readFile, rm } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const source = join(root, 'extensions', 'chromium-standalone');
const manifest = JSON.parse(await readFile(join(source, 'manifest.json'), 'utf8'));
const directory = join(root, 'dist', 'standalone-extension');
const output = join(directory, `banana-hand-standalone-${manifest.version}.zip`);
await mkdir(directory, { recursive: true });
await rm(output, { force: true });
execFileSync('python3', ['-c', `
import pathlib, sys, zipfile
source, output = map(pathlib.Path, sys.argv[1:])
with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(source.rglob('*')):
        if path.is_file():
            archive.write(path, path.relative_to(source))
print(output)
`, source, output], { stdio: 'inherit' });
