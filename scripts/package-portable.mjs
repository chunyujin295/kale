import { chmod, cp, mkdir, rm, writeFile } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const platformNames = { win32: 'win', darwin: 'macos', linux: 'linux' };
const platform = platformNames[process.platform];
if (!platform) throw new Error(`Unsupported platform: ${process.platform}`);
const output = join(projectRoot, 'dist', `kale-${platform}-${process.arch}`);
const nodeRuntime = process.execPath;

await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });
await cp(nodeRuntime, join(output, basename(nodeRuntime)));
await cp(join(projectRoot, 'src'), join(output, 'src'), { recursive: true });
await cp(join(projectRoot, 'node_modules'), join(output, 'node_modules'), { recursive: true });
if (process.platform === 'win32') {
  await writeFile(join(output, 'kale.cmd'), '@echo off\r\n"%~dp0node.exe" "%~dp0src\\cli.js" %*\r\n', 'utf8');
} else {
  const launcher = join(output, 'kale');
  await writeFile(launcher, '#!/usr/bin/env sh\nSCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"\nexec "$SCRIPT_DIR/node" "$SCRIPT_DIR/src/cli.js" "$@"\n', 'utf8');
  await chmod(launcher, 0o755);
}
await writeFile(join(output, 'README.txt'), 'Run the kale launcher with an image path and options. Keep this folder intact because Sharp uses native image decoding libraries.\n', 'utf8');
console.log(`Created portable package: ${output}`);
