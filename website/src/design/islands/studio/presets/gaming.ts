import { Gamepad2 } from 'lucide-react';
import { named, type PresetDef } from './shared';

export const gamingPreset: PresetDef = {
  id: 'gaming',
  label: 'Gaming',
  blurb: 'Steam on Mesa, Xwayland + gamepad',
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
      'noto-fonts',
      'noto-fonts-emoji',
      'noto-fonts-cjk',
    ],
    runCommands: ['rm -rf /var/cache/pacman/pkg/*'],
    // No shell override: bash is the default, and Steam launches from a
    // .desktop entry that never sees an interactive shell.
    // Game library on a real disk, plus the host Xwayland socket for the
    // clients that still speak X. DISPLAY has to match the host's.
    extraMounts: [
      { host: '~/Games', guest: '/home/%u/Games', mode: 'z' },
      { host: '/tmp/.X11-unix', guest: '/tmp/.X11-unix', mode: 'z' },
    ],
    envVars: [{ key: 'DISPLAY', value: ':0' }],
    // Steam's runtime sandbox needs syscalls the default profile blocks;
    // tighten back to "default" if your library runs without it.
    seccomp: 'unconfined',
    netMode: 'pasta',
    intWayland: true,
    intAudio: true,
    intDbus: true,
    intNotify: true,
    intXdgOpen: true,
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
