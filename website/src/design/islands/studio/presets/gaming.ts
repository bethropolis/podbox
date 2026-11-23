import { Gamepad2 } from 'lucide-react';
import { named, type PresetDef } from './shared';

export const gamingPreset: PresetDef = {
  id: 'gaming',
  label: 'Gaming',
  blurb: 'Steam on Mesa, X11 bridge + gamepad',
  icon: Gamepad2,
  accent: 'text-[var(--accent-blue)]',
  overrides: {
    selectedPresetDistro: 'cachyos/cachyos:latest',
    ...named('gaming'),
    // Driver pins: steam only asks for the virtual "vulkan-driver" /
    // "opengl-driver" provides, so Mesa has to be named outright. These are
    // the AMD set; Intel swaps vulkan-radeon for vulkan-intel (both 64- and
    // lib32-).
    packagesInstallList: [
      'steam',
      'gamescope',
      'mangohud',
      'lib32-mangohud',
      'gamemode',
      'lib32-gamemode',
      'mesa',
      'lib32-mesa',
      'vulkan-radeon',
      'lib32-vulkan-radeon',
      'vulkan-tools',
      'mesa-utils',
      'xorg-xdpyinfo',
      'pipewire-alsa',
    ],
    // No cache-clean RUN step: the engine appends `pacman -Scc` to the
    // install layer itself, so a separate step would only add a layer.
    // No shell override: bash is the default, and Steam launches from a
    // .desktop entry that never sees an interactive shell.
    // Game library on a real disk; X11 clients use podbox's own bridge.
    extraMounts: [{ host: '~/Games', guest: '/home/%u/Games', mode: 'z' }],
    // Steam's runtime sandbox needs syscalls the default profile blocks;
    // tighten back to "default" if your library runs without it.
    seccomp: 'unconfined',
    netMode: 'pasta',
    intWayland: true,
    intX11: true,
    intAudio: true,
    intDbus: true,
    intNotify: true,
    intXdgOpen: true,
    intSyncFonts: true,
    hwJoystick: true,
    exportAppsList: ['steam'],
    // Studio defaults share Downloads/Projects and enable auto-update; the
    // spec shares nothing and leaves updates manual.
    xdgDownloads: 'off',
    xdgProjects: 'off',
    lifeAutoUpdate: false,
    lifeQuadlet: true,
    lifeAutoCheckpoint: true,
  },
};
