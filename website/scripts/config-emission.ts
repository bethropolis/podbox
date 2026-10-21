// Headless emission probe: feed a podbox.toml through the same wasm engine
// the website and CLI share, and print what comes out — no browser needed.
//
//   bun scripts/config-emission.ts                # built-in sample
//   bun scripts/config-emission.ts my.toml        # your file
//   echo '[image]...' | bun scripts/config-emission.ts -
//
//(exit != 0 when the engine reports validation errors)

import { ensureEngine, parseTomlText, configToToml, validateToml, compileQuadletToml } from '../src/design/islands/studio/engine';

const SAMPLE = `\
[image]
base = "ubuntu:26.04"
name = "dev-box"

[container]
name = "dev-box"
home = "~/containers/dev-box"

[image.packages]
install = ["neovim", "ripgrep", "git", "fish"]

[integration]
wayland = true
audio = true
dbus = true

[integration.xdg_dirs.projects]
enabled = true
read_write = true
`;

await ensureEngine();

const arg = process.argv[2];
const text = !arg || arg === '-'
  ? await Bun.stdin.text().then((t) => t || SAMPLE)
  : await Bun.file(arg).text();

const parsed = parseTomlText(text);
if (!parsed) {
  console.error('parse: FAILED (not a config podbox understands)');
  process.exit(2);
}
console.log('— emitted —');
const emitted = configToToml(parsed as Record<string, unknown>) ?? '(config_to_toml returned null)';
console.log(emitted);

const report = validateToml(emitted);
console.log(`— validation — valid=${report?.valid ? 'yes' : 'NO'}`);
for (const e of report?.errors ?? []) {
  console.log(`  error: ${e.field ? `${e.field}: ` : ''}${e.message}${e.line ? ` (line ${e.line})` : ''}`);
}
for (const a of report?.advisories ?? []) {
  console.log(`  advisory: ${a.field ? `${a.field}: ` : ''}${a.message}`);
}
for (const w of report?.warnings ?? []) console.log(`  warning: ${w}`);

const quadlet = compileQuadletToml(emitted);
if (quadlet) {
  console.log(`— quadlet (${quadlet.container.split('\n').length} lines) —`);
  console.log(quadlet.container);
}

if (report && !report.valid) process.exit(1);
