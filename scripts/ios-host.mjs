import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const action = process.argv[2];
if (!['init', 'dev'].includes(action)) throw Error('Usage: node scripts/ios-host.mjs init|dev [CLI dev arguments]');
if (process.platform !== 'darwin') throw Error('The complete iOS host requires macOS/Xcode; Linux cannot run the locked Tauri iOS CLI.');
const version = execFileSync('pnpm', ['tauri', '--version'], { cwd: root, encoding: 'utf8' });
if (!version.includes('tauri-cli 2.11.4')) throw Error('Use pnpm install --frozen-lockfile: expected locked Tauri CLI 2.11.4');
if (action === 'init') {
  for (const command of ['xcodebuild', 'xcodegen', 'pod']) execFileSync('which', [command], { stdio: 'ignore' });
  // Standard generator owns Sources, project.yml, pbxproj, plugin links and assets.
  // Never hand-roll an alternate Swift UI or replace an existing generated project.
  execFileSync('pnpm', ['tauri', 'ios', 'init', '--ci', '--skip-targets-install'], { cwd: root, stdio: 'inherit' });
  if (!existsSync(new URL('../src-tauri/gen/apple/project.yml', import.meta.url))) throw Error('Tauri did not generate the Apple project');
} else {
  if (!existsSync(new URL('../src-tauri/gen/apple/project.yml', import.meta.url))) throw Error('Run pnpm ios:init first');
  const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.ios.debug.conf.json', import.meta.url)));
  if (config.app.windows[0].devtools !== true) throw Error('Debug inspector configuration missing');
  execFileSync('pnpm', ['tauri', 'ios', 'dev', '--config', 'src-tauri/tauri.ios.debug.conf.json', ...process.argv.slice(3)], { cwd: root, stdio: 'inherit' });
}
