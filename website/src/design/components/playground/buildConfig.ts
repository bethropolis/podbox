// Playground controls -> Config-shaped JSON for the wasm engine.
//
// Only the shape is built here: TOML and Quadlet emission live in Rust
// (engine.ts), so the homepage playground shows exactly what the CLI would
// read — the same single source the Studio uses, with no TS replica to drift.

type V = Record<string, any>;

// Mirror the Studio's preset list (schema.ts IMAGE_PRESET_OPTIONS): the four
// playground distro buttons map to stable base refs, `custom` passes the
// typed image through verbatim.
const DISTRO_BASES: Record<string, string> = {
  fedora: 'fedora:44',
  arch: 'archlinux:latest',
  ubuntu: 'ubuntu:26.04',
  debian: 'debian:trixie',
};

export interface PlaygroundState {
  name: string;
  distro: 'fedora' | 'arch' | 'ubuntu' | 'debian' | 'custom';
  customImage: string;
  /** Comma-separated package list, editable as one string in the UI. */
  packages: string;
  wayland: boolean;
  pipewire: boolean;
  gpu: 'auto' | 'nvidia' | 'amd' | 'intel' | 'off';
  dbusNotifications: boolean;
  shareProjects: boolean;
}

export function playgroundConfig(s: PlaygroundState): V {
  const image: V = {
    // The name rides on the container's so one rename updates both; the
    // serializer drops it when it equals the engine default.
    name: s.name,
    base: s.distro === 'custom' ? s.customImage : DISTRO_BASES[s.distro],
  };
  const install = s.packages.split(',').map((p) => p.trim()).filter(Boolean);
  if (install.length > 0) image.packages = { install };

  // gpu: `off` is the engine's bare false, `auto` rides along as the default
  // token (dropped by the serializer), real modes pass their string.
  const gpu = s.gpu === 'off' ? false : s.gpu;

  return {
    image,
    container: {
      name: s.name,
      home: `~/containers/${s.name}`,
    },
    integration: {
      wayland: s.wayland,
      audio: s.pipewire,
      gpu,
      dbus: s.dbusNotifications,
      xdg_dirs: {
        projects: s.shareProjects ? { enabled: true, read_write: true } : false,
      },
    },
  };
}
