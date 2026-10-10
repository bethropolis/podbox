import { Monitor } from 'lucide-react';
import { named, type PresetDef } from './shared';

export const archGuiPreset: PresetDef = {
  id: 'arch-gui',
  label: 'Arch GUI',
  blurb: 'Foot, Firefox, Wayland',
  icon: Monitor,
  accent: 'text-[var(--accent-blue)]',
  overrides: {
    // Vanilla Arch: repos only — no AUR helper ships upstream, and none is
    // preinstalled (git/base-devel were trimmed; the AUR story stays manual
    // until someone actually needs it).
    selectedPresetDistro: 'archlinux:latest',
    ...named('arch'),
    // foot: Wayland-native terminal, no GPU needed. firefox: the reference
    // GUI app — proves the whole desktop bridge on first boot.
    // mesa + vulkan-icd-loader:
    // GL/Vulkan behind the auto /dev/dri passthrough (add vulkan-radeon or
    // vulkan-intel later if a Vulkan app complains — host-GPU-specific, so
    // not pinned here). pipewire + libpulse: client libs for host-provided
    // audio (the daemon runs on the host). dejavu + jetbrains-mono +
    // nerd-symbols: small in-image set (fallback breadth, terminal font,
    // prompt icons). No noto, no host font sync: fontconfig resolves
    // monospace correctly from the image set alone, and syncing host fonts
    // just shadows it.
    // wl-clipboard is deliberately absent: clipboard=true enables the
    // bridge capability and GUI toolkits speak the data-device protocol
    // directly — the wl-copy binaries are only for manual terminal use.
    // xdg-utils provides xdg-open; dbus gives dbus-send /
    // dbus-run-session for portal calls and debugging the dbus bridge.
    // git is deliberately absent: nothing in this preset clones, and the
    // AUR story stays manual until someone actually needs it.
    // Passwordless sudo needs no config: the guest entrypoint writes it at
    // every boot. The only knob is noNewPrivileges below — sudo refuses to
    // run under it at all.
    // No cache-clean RUN step: the engine appends `pacman -Scc` itself.
    // pipewire-jack pre-empts firefox's provider prompt (jack2 vs
    // pipewire-jack): without it a non-interactive build silently takes
    // jack2, a second audio stack next to pipewire.
    packagesInstallList: [
      'foot',
      'firefox',
      'mesa',
      'vulkan-icd-loader',
      'libpulse',
      'pipewire',
      'pipewire-jack',
      'fontconfig',
      'ttf-dejavu',
      'ttf-jetbrains-mono',
      'ttf-nerd-fonts-symbols',
      'xdg-utils',
      'dbus',
      // Fish is the container shell below, so it must be in the image:
      // `enter` execs it directly and fails without it.
      'fish',
    ],
    containerShell: 'fish',
    // No SHELL env row: the engine derives it from the shell above.
    intWayland: true,
    intAudio: true,
    intDbus: true,
    intNotify: true,
    intXdgOpen: true,
    intClipboard: true,
    // No font sync: the in-image set resolves monospace on its own, and
    // host fonts only shadow it (that was the Plex Sans warning).
    intSyncIcons: true,
    intSyncThemes: true,
    xdgDownloads: 'ro',
    xdgDesktop: 'rw',
    // Default is 'rw' — this preset deliberately does not share ~/Projects.
    xdgProjects: 'off',
    exportAppsList: ['foot', 'firefox'],
    // sudo refuses to run at all under no-new-privileges; passwordless
    // itself comes free from the guest entrypoint. Deliberate tradeoff for
    // an interactive GUI box (still userns-contained).
    noNewPrivileges: false,
    lifeQuadlet: true,
    lifeAutoUpdate: true,
    lifeIdleTimeout: '5m',
  },
};
