// Copies the Whisper engine (bin/) and models (models/) into
// src-tauri/resources/ so the NSIS/MSI installer bundles them and the app is
// fully self-contained when installed on a fresh machine.
import { existsSync, readdirSync, statSync, rmSync, mkdirSync, copyFileSync, unlinkSync } from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const here = path.dirname(url.fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..');
const destRoot = path.join(repoRoot, 'src-tauri', 'resources');

const pairs = [
  ['models', 'resources/models'],
  ['bin', 'resources/bin'],
];

function filesUnder(dir) {
  const out = [];
  (function walk(d) {
    for (const name of readdirSync(d)) {
      const entry = path.join(d, name);
      if (statSync(entry).isDirectory()) walk(entry);
      else out.push(entry);
    }
  })(dir);
  return out;
}

for (const [srcDir, destDir] of pairs) {
  const src = path.join(repoRoot, srcDir);
  const dest = path.join(repoRoot, 'src-tauri', destDir);
  if (!existsSync(src)) {
    console.log(`[bundle-resources] skipping ${srcDir} (not present)`);
    continue;
  }
  const srcFiles = filesUnder(src);
  if (srcFiles.length === 0) {
    console.log(`[bundle-resources] skipping ${srcDir} (empty)`);
    continue;
  }

  mkdirSync(dest, { recursive: true });
  const destFiles = existsSync(dest) ? filesUnder(dest) : [];
  const destSet = new Set(destFiles);

  let copied = 0;
  let bytes = 0;
  for (const file of srcFiles) {
    const rel = path.relative(src, file);
    const target = path.join(dest, rel);
    if (!existsSync(target) || statSync(target).size !== statSync(file).size) {
      mkdirSync(path.dirname(target), { recursive: true });
      copyFileSync(file, target);
      copied++;
      bytes += statSync(file).size;
    }
    destSet.delete(target);
  }
  for (const stale of destSet) {
    unlinkSync(stale);
  }
  console.log(`[bundle-resources] ${srcDir} -> ${destDir} (${copied} copied, ${(bytes / 1024 / 1024).toFixed(1)} MB)`);
}
