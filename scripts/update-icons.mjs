#!/usr/bin/env node
/**
 * Regenerates every platform icon from a single source PNG.
 *
 * `tauri icon` produces the desktop set plus, in the same run, the Android and
 * iOS sets. Only the desktop one is where it belongs: the Android set has to be
 * copied into the generated Android project, and the iOS set is not used by
 * this app at all. Doing that by hand is five steps with two traps, which is
 * why it lives here.
 *
 * Usage:
 *   npm run icons
 *   npm run icons -- --source path/to/other.png
 *   npm run icons -- --bg '#AABFF5'
 *
 * The two traps this handles:
 *
 *  1. `tauri icon` always writes the Android adaptive icon's background as
 *     Tauri's default `#FFFFFF`. For a coloured icon that shows up as a white
 *     halo wherever the launcher masks the icon. Pass the icon's own edge
 *     colour with `--bg`.
 *  2. It drops `android/` and `ios/` subdirectories into the *desktop* icons
 *     directory, which then get bundled for no reason.
 */

import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');

/** Default background for the Android adaptive icon. */
const DEFAULT_BACKGROUND = '#AABFF5';

const paths = {
  desktopApp: join(root, 'apps', 'desktop'),
  desktopIcons: join(root, 'apps', 'desktop', 'src-tauri', 'icons'),
  androidIcons: join(root, 'apps', 'android', 'src-tauri', 'icons'),
  androidRes: join(root, 'apps', 'android', 'src-tauri', 'gen', 'android', 'app', 'src', 'main', 'res'),
};

function parseArgs(argv) {
  const options = { source: join(root, 'icon.png'), background: DEFAULT_BACKGROUND };

  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    if (flag === '--source' && argv[i + 1]) {
      options.source = resolve(process.cwd(), argv[++i]);
    } else if (flag === '--bg' && argv[i + 1]) {
      options.background = argv[++i];
    } else {
      console.error(`unknown argument: ${flag}`);
      process.exit(2);
    }
  }

  if (!/^#[0-9a-fA-F]{6}$/.test(options.background)) {
    console.error(`--bg must look like #RRGGBB, got ${options.background}`);
    process.exit(2);
  }

  return options;
}

function fail(message) {
  console.error(`\n  error: ${message}\n`);
  process.exit(1);
}

const options = parseArgs(process.argv.slice(2));

if (!existsSync(options.source)) {
  fail(`no source image at ${options.source}`);
}

console.log(`source     : ${options.source}`);
console.log(`background : ${options.background}  (Android adaptive icon)\n`);

// ---------------------------------------------------------------------------
// 1. Let the Tauri CLI produce every format it knows about.
// ---------------------------------------------------------------------------

const cli = require.resolve('@tauri-apps/cli/tauri.js');

console.log('· generating icons with the Tauri CLI');
execFileSync(process.execPath, [cli, 'icon', options.source], {
  cwd: paths.desktopApp,
  stdio: 'inherit',
});

// ---------------------------------------------------------------------------
// 2. Give the Android host its own copy of the desktop formats.
//    Its tauri.conf.json lists the same bundle icons, and `tauri android`
//    reads them from here rather than from the desktop project.
// ---------------------------------------------------------------------------

if (!existsSync(paths.androidIcons)) {
  mkdirSync(paths.androidIcons, { recursive: true });
}

let copied = 0;
for (const entry of readdirSync(paths.desktopIcons)) {
  if (!/\.(png|ico|icns)$/i.test(entry)) continue;
  cpSync(join(paths.desktopIcons, entry), join(paths.androidIcons, entry));
  copied++;
}
console.log(`\n· copied ${copied} files to apps/android/src-tauri/icons`);

// ---------------------------------------------------------------------------
// 3. The Android adaptive icon goes into the generated project.
// ---------------------------------------------------------------------------

const generatedAndroid = join(paths.desktopIcons, 'android');
if (!existsSync(generatedAndroid)) {
  fail(`the Tauri CLI produced no android/ directory in ${paths.desktopIcons}`);
}

cpSync(generatedAndroid, paths.androidRes, { recursive: true });
console.log(`· copied the adaptive icon set into gen/android/.../res`);

// ---------------------------------------------------------------------------
// 4. ... with a background that matches the artwork instead of Tauri's white.
// ---------------------------------------------------------------------------

const backgroundXml = join(paths.androidRes, 'values', 'ic_launcher_background.xml');
mkdirSync(dirname(backgroundXml), { recursive: true });

// XML forbids two consecutive hyphens inside a comment, so the command line is
// described rather than quoted. Writing `npm run icons -- --bg '#RRGGBB'` here
// used to break the Android build with:
//
//   res/values/ic_launcher_background.xml:8:20: Error: The string "--" is not
//   allowed in comments.
//
// ...which only showed up much later, at :app:mergeUniversalDebugResources.
// The guard below keeps that from coming back.
const backgroundComment = [
  '  Background layer of the Android adaptive icon.',
  '',
  "  Generated by scripts/update-icons.mjs. Tauri's default for this file is",
  '  #FFFFFF, which shows as a white halo around a coloured icon wherever the',
  '  launcher masks it; the value below is sampled from the source artwork.',
  '',
  '  Regenerating with a different colour is documented in docs/MAINTENANCE.md,',
  '  section 6. The command is not repeated here because XML forbids two',
  '  consecutive hyphens inside a comment.',
].join('\n');

if (backgroundComment.includes('--')) {
  throw new Error('the XML comment must not contain two consecutive hyphens');
}

writeFileSync(
  backgroundXml,
  `<?xml version="1.0" encoding="utf-8"?>
<!--
${backgroundComment}
-->
<resources>
  <color name="ic_launcher_background">${options.background}</color>
</resources>
`,
);
console.log(`· set the adaptive icon background to ${options.background}`);

// ---------------------------------------------------------------------------
// 5. The desktop project has no use for the mobile sets.
// ---------------------------------------------------------------------------

for (const directory of ['android', 'ios']) {
  const target = join(paths.desktopIcons, directory);
  if (!existsSync(target)) continue;
  if (!statSync(target).isDirectory()) continue;
  rmSync(target, { recursive: true, force: true });
  console.log(`· removed the unused ${directory}/ set from the desktop icons`);
}

console.log('\ndone. Check the result with:');
console.log('  git status -- apps/desktop/src-tauri/icons apps/android/src-tauri');
