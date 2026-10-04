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
import { STUDIO_DEFAULTS } from "../src/design/islands/studio/schema";
import type { StudioValues } from "../src/design/islands/studio/useStudioState";

await init();

// Defaults live in the shared schema module (same object the UI boots
// from), so this guard tests exactly what users see.
const defaults: StudioValues = { ...STUDIO_DEFAULTS };

const cases: Array<[string, Partial<StudioValues>]> = [
  ["defaults", {}],
  ["rust", presetPatch("rust")],
  ["arch-gui", presetPatch("arch-gui")],
  ["fullstack", presetPatch("fullstack")],
  ["minimal", presetPatch("minimal")],
  // New panels: dotfiles + caches + services + slice/weight + offline.
  [
    "integrations",
    {
      dotfilesSource: "host:~/.dotfiles",
      dotfilesTarget: "~/.dotfiles",
      dotfilesCloneOn: "container",
      dotfilesInstall: "./install.sh",
      sharedCaches: ["cargo", "npm"],
      hostCaches: ["pip"],
      services: [
        { name: "redis", command: "redis-server", restart: "on-failure" },
        { name: "worker", command: "npm run worker", restart: "always" },
      ],
      containerSlice: "podbox.slice",
      containerCpuWeight: 500,
      netOffline: true,
      intGitIdentity: false,
      lifeAutoCheckpoint: true,
    },
  ],
];

let failed = 0;
for (const [name, patch] of cases) {
  const values = { ...defaults, ...patch };
  const toml = generateStudioToml(values);
  const v = validate_toml(toml) as {
    valid: boolean;
    errors: Array<{ field?: string; message: string }>;
    warnings: string[];
  };
  if (!v.valid) {
    console.error(
      `✗ ${name}: TOML rejected by Rust engine:\n  - ${v.errors.map((e) => (e.field ? `${e.field}: ${e.message}` : e.message)).join("\n  - ")}`,
    );
    failed++;
    continue;
  }
  let compiled: { container: string; containerfile: string | null; warnings: string[] };
  try {
    compiled = compile_quadlet(toml) as { container: string; containerfile: string | null; warnings: string[] };
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
  // Every config yields a Containerfile. A custom base renders the full
  // recipe with the guest layer marked as a placeholder (wasm embeds no
  // guest binary) plus a warning; prebuilt images get the short overlay.
  const cf = compiled.containerfile as string | undefined;
  if (!cf || !cf.startsWith('FROM ')) {
    console.error(`✗ ${name}: expected a Containerfile starting with FROM`);
    failed++;
    continue;
  }
  const guestMarked = cf.includes('podbox-guest: not embedded in this build');
  const guestCopied = cf.includes('COPY podbox-guest');
  const prebuilt = values.imagePrebuiltRef !== '';
  if (prebuilt) {
    if (guestCopied) {
      console.error(`✗ ${name}: prebuilt overlay must not COPY the guest`);
      failed++;
      continue;
    }
  } else if (!guestMarked || guestCopied) {
    console.error(`✗ ${name}: custom base must mark the guest layer instead of copying it`);
    failed++;
    continue;
  }
  // The placeholder describes the preview environment, so it belongs in the
  // rendered text and must never reach the user as a config warning.
  if (compiled.warnings.some((w) => w.toLowerCase().includes('guest'))) {
    console.error(`✗ ${name}: guest placeholder must not be reported as a warning`);
    failed++;
    continue;
  }
  // Import loop: generated TOML -> state patch -> regenerated TOML must
  // still validate (guards parseImport.ts against schema drift too).
  const reimported = { ...values, ...tomlToPatch(parseTomlText(toml) as Record<string, any>) };
  const toml2 = generateStudioToml(reimported);
  const v2 = validate_toml(toml2) as {
    valid: boolean;
    errors: Array<{ field?: string; message: string }>;
  };
  if (!v2.valid) {
    console.error(
      `✗ ${name}: re-imported TOML rejected:\n  - ${v2.errors.map((e) => (e.field ? `${e.field}: ${e.message}` : e.message)).join("\n  - ")}`,
    );
    failed++;
    continue;
  }
  console.log(`✓ ${name}: valid TOML, compiled ${compiled.container.split("\n").length} lines`);
}

if (failed > 0) {
  console.error(`\n${failed} case(s) failed — Studio and podbox schema have drifted.`);
  process.exit(1);
}

// Structured errors carry field targets for input highlighting.
{
  const bad = { ...defaults, containerMemory: "bogus" };
  const v = validate_toml(generateStudioToml(bad)) as {
    valid: boolean;
    errors: Array<{ field?: string; message: string }>;
  };
  const memErr = v.errors.find((e) => e.field === "container.memory");
  if (v.valid || !memErr) {
    console.error("✗ field-errors: expected a container.memory-targeted issue");
    process.exit(1);
  }
  console.log(`✓ field-errors: container.memory -> "${memErr.message}"`);
}
// Prebuilt-image configs take the overlay path, which the wasm build CAN
// render (no guest binary needed).
{
  const values = { ...defaults, imagePrebuiltRef: "ghcr.io/bethropolis/podbox:cachy-latest" };
  const toml = generateStudioToml(values);
  const compiled = compile_quadlet(toml) as { containerfile: string | null };
  if (!compiled.containerfile?.includes("FROM")) {
    console.error("✗ prebuilt: expected a Containerfile overlay");
    process.exit(1);
  }
  console.log(`✓ prebuilt: Containerfile overlay rendered (${compiled.containerfile.split("\n").length} lines)`);
}
console.log("\nAll Studio presets round-trip through the Rust engine.");
