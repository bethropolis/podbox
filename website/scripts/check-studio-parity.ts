// Round-trip guard: every Studio preset (plus defaults) must generate TOML
// that the real Rust engine accepts, and must compile to a Quadlet unit.
//
// Catches Studio↔podbox schema drift: if generateToml.ts emits a field the
// Rust parser rejects (or drops), this fails.
//
// Run: bun scripts/check-studio-parity.ts
// (requires website/src/wasm built via ../scripts/build-wasm.sh)
import init, { validate_toml, compile_quadlet } from "../src/wasm/podbox_wasm.js";
import { parse as parseTomlText } from "smol-toml";
import { generateStudioToml } from "../src/design/islands/studio/generateToml";
import { tomlToPatch } from "../src/design/islands/studio/parseImport";
import { presetPatch } from "../src/design/islands/studio/presets";
import type { StudioValues } from "../src/design/islands/studio/useStudioState";

await init();

// Mirror of the useState initializers in useStudioState.ts (config values
// only — view state excluded, same as the persistable subset).
const defaults: StudioValues = {
  activePreset: "custom",
  imageType: "preset",
  selectedPresetDistro: "fedora:44",
  customImageBase: "ghcr.io/username/custom-env:latest",
  imageName: "dev-box",
  imagePrebuiltRef: "",
  pullRetry: 3,
  pullRetryDelay: "5s",
  packagesInstallList: ["git"],
  packagesRemoveList: [],
  packageManager: "auto",
  runCommands: "dnf clean all",
  containerName: "dev-box",
  containerHome: "~/containers/dev-box",
  containerShell: "bash",
  containerMemory: "4G",
  containerCpus: "2.0",
  containerReloadCmd: "",
  extraMounts: [],
  envVars: [],
  apparmor: "",
  seccomp: "default",
  secLabelDisable: true,
  noNewPrivileges: true,
  readOnlyRootfs: false,
  usernsMode: "keep-id",
  capPreset: "default",
  extraCapAddList: [],
  netMode: "private",
  portMappingsList: [],
  intWayland: true,
  intAudio: true,
  intGpu: "auto",
  intDbus: false,
  intNotify: false,
  intXdgOpen: false,
  intClipboard: false,
  intSyncFonts: false,
  intSyncIcons: false,
  intSyncThemes: false,
  intSshAgent: false,
  intGpgAgent: false,
  hostExecEnabled: false,
  hostExecList: [],
  xdgDocuments: false,
  xdgDownloads: false,
  xdgPictures: false,
  xdgMusic: false,
  xdgVideos: false,
  xdgDesktop: false,
  xdgProjects: false,
  exportAppsList: [],
  exportBinsList: [],
  lifeQuadlet: true,
  lifeAutostart: false,
  lifeOnStop: "keep",
  lifeAutoUpdate: false,
  lifeIdleTimeout: "off",
  sysRequires: "",
  sysAfter: "network-online.target",
  dbusPreset: "portal",
  dbusTalkList: [],
  dbusOwnList: [],
  waylandFirewall: false,
  waylandBlockedList: [],
} as StudioValues;

const cases: Array<[string, Partial<StudioValues>]> = [
  ["defaults", {}],
  ["rust", presetPatch("rust")],
  ["arch-gui", presetPatch("arch-gui")],
  ["fullstack", presetPatch("fullstack")],
  ["minimal", presetPatch("minimal")],
];

let failed = 0;
for (const [name, patch] of cases) {
  const values = { ...defaults, ...patch };
  const toml = generateStudioToml(values);
  const v = validate_toml(toml) as { valid: boolean; errors: string[]; warnings: string[] };
  if (!v.valid) {
    console.error(`✗ ${name}: TOML rejected by Rust engine:\n  - ${v.errors.join("\n  - ")}`);
    failed++;
    continue;
  }
  let compiled: { container: string };
  try {
    compiled = compile_quadlet(toml) as { container: string };
  } catch (e) {
    console.error(`✗ ${name}: compile threw: ${e}`);
    failed++;
    continue;
  }
  if (!compiled.container.includes(`Description=podbox -- ${values.containerName}`)) {
    console.error(`✗ ${name}: compiled unit missing container section`);
    failed++;
    continue;
  }
  // Import loop: generated TOML -> state patch -> regenerated TOML must
  // still validate (guards parseImport.ts against schema drift too).
  const reimported = { ...values, ...tomlToPatch(parseTomlText(toml) as Record<string, any>) };
  const toml2 = generateStudioToml(reimported);
  const v2 = validate_toml(toml2) as { valid: boolean; errors: string[] };
  if (!v2.valid) {
    console.error(`✗ ${name}: re-imported TOML rejected:\n  - ${v2.errors.join("\n  - ")}`);
    failed++;
    continue;
  }
  console.log(`✓ ${name}: valid TOML, compiled ${compiled.container.split("\n").length} lines`);
}

if (failed > 0) {
  console.error(`\n${failed} case(s) failed — Studio and podbox schema have drifted.`);
  process.exit(1);
}
console.log("\nAll Studio presets round-trip through the Rust engine.");
