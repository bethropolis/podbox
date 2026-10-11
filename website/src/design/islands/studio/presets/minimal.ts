import { Lock } from 'lucide-react';
import { headlessIntegration, named, type PresetDef } from './shared';

/**
 * Hardened: a usable dev box with every host-sharing surface off.
 *
 * The opinionated parts are the four that actually reduce blast radius:
 *
 * - `integration.*` all off, so no socket, device, clipboard, or D-Bus
 *   crosses the boundary. Anything host-shaped is a deliberate edit away
 *   rather than a default you have to notice and remove.
 * - `cap_preset = "none"` with an empty `cap_add`: rootless user namespacing
 *   keeps the container working without capability grants.
 * - `network.offline` keeps `Network=none` while the chosen `mode` survives
 *   in the config, so turning networking on later is one flip instead of a
 *   rewrite. Ports cannot be published while offline — the port list is
 *   emptied to match.
 * - `read_only_rootfs` makes `/` immutable; `~/containers/hardened` is the
 *   home volume, so shells, editors and scratch files keep working. The
 *   generator adds tmpfs for `/run`, `/tmp` and `/var/tmp` — the guest daemon
 *   writes `/run/podbox/bin` before it can talk to the host at all.
 * - `no_new_privileges` blocks setuid escalation, so the `sudo` this image
 *   installs cannot elevate. That is the point of the flag: work as the
 *   mapped user and there is nothing to escalate to.
 */
export const minimalPreset: PresetDef = {
  id: 'minimal',
  label: 'Hardened',
  blurb: 'Offline, no caps, immutable root',
  icon: Lock,
  accent: 'text-[var(--accent-red)]',
  overrides: {
    selectedPresetDistro: 'archlinux:latest',
    ...named('hardened'),
    packagesInstallList: ['git', 'fish', 'neovim', 'ripgrep', 'fd'],
    packageManager: 'pacman',
    containerShell: 'fish',
    // A container that needs 4G or 4 cores to run an editor is a smell, but
    // the caps keep a runaway build from consuming the machine.
    containerMemory: '4G',
    containerCpus: '2.0',
    runCommands: ['pacman -Scc --noconfirm'],
    ...headlessIntegration,
    intDbus: false,
    intNotify: false,
    intXdgOpen: false,
    intClipboard: false,
    intSshAgent: false,
    intGpgAgent: false,
    intGitIdentity: false,
    intSyncFonts: false,
    intSyncIcons: false,
    intSyncThemes: false,
    hostExecEnabled: false,
    // XDG dir shares are host surfaces like any other mount, and the Studio
    // defaults share Downloads and Projects. A hardened box shares nothing.
    xdgDocuments: 'off',
    xdgDownloads: 'off',
    xdgPictures: 'off',
    xdgMusic: 'off',
    xdgVideos: 'off',
    xdgDesktop: 'off',
    xdgProjects: 'off',
    secLabelDisable: false,
    noNewPrivileges: true,
    readOnlyRootfs: true,
    capPreset: 'none',
    extraCapAddList: [],
    // Offline wins over the mode for the generated unit; `none` is what the
    // offline flag renders as, so the two agree in the preview.
    netMode: 'none',
    netOffline: true,
    portMappingsList: [],
    lifeAutostart: false,
    lifeOnStop: 'remove',
    lifeAutoUpdate: false,
  },
};
