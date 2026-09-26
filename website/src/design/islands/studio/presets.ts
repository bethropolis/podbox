import type { StudioValues } from './useStudioState';

export function presetPatch(preset: 'rust' | 'arch-gui' | 'fullstack' | 'minimal'): Partial<StudioValues> {
  const patch: Partial<StudioValues> = {};
    patch.activePreset = preset;
    if (preset === 'rust') {
      patch.imageType = 'preset';
      patch.selectedPresetDistro = 'fedora:44';
      patch.containerName = 'rust-dev';
      patch.imageName = 'rust-dev';
      patch.containerHome = '~/containers/rust-dev';
      patch.packagesInstallList = ['cargo', 'rustc', 'neovim', 'ripgrep', 'git', 'mold'];
      patch.intWayland = true;
      patch.intAudio = true;
      patch.intGpu = 'auto';
      patch.intSshAgent = true;
      patch.netMode = 'private';
      patch.lifeQuadlet = true;
    } else if (preset === 'arch-gui') {
      patch.imageType = 'preset';
      patch.selectedPresetDistro = 'archlinux:latest';
      patch.containerName = 'arch-desktop';
      patch.imageName = 'arch-desktop';
      patch.containerHome = '~/containers/arch-desktop';
      patch.packagesInstallList = ['firefox', 'alacritty', 'neovim', 'mesa', 'pipewire'];
      patch.intWayland = true;
      patch.intAudio = true;
      patch.intGpu = 'auto';
      patch.intClipboard = true;
      patch.dbusPreset = 'portal';
      patch.lifeQuadlet = true;
    } else if (preset === 'fullstack') {
      patch.imageType = 'preset';
      patch.selectedPresetDistro = 'ubuntu:24.04';
      patch.containerName = 'fullstack-web';
      patch.imageName = 'fullstack-web';
      patch.containerHome = '~/containers/fullstack-web';
      patch.packagesInstallList = ['nodejs', 'npm', 'pnpm', 'git', 'curl', 'python3'];
      patch.portMappingsList = ['3000:3000', '5173:5173', '8080:8080'];
      patch.netMode = 'pasta';
      patch.intWayland = false;
      patch.intAudio = false;
      patch.intGpu = 'false';
      patch.lifeAutostart = true;
    } else if (preset === 'minimal') {
      patch.imageType = 'preset';
      patch.selectedPresetDistro = 'alpine:3.20';
      patch.containerName = 'micro-box';
      patch.imageName = 'micro-box';
      patch.containerHome = '~/containers/micro-box';
      patch.packagesInstallList = ['busybox-extras', 'curl', 'ca-certificates'];
      patch.intWayland = false;
      patch.intAudio = false;
      patch.intGpu = 'false';
      patch.intDbus = false;
      patch.readOnlyRootfs = true;
      patch.noNewPrivileges = true;
      patch.netMode = 'none';
    }
  return patch;
}

export function defaultPatch(): Partial<StudioValues> {
  const patch: Partial<StudioValues> = {};
    patch.activePreset = 'custom';
    patch.imageType = 'preset';
    patch.selectedPresetDistro = 'fedora:44';
    patch.containerName = 'dev-box';
    patch.imageName = 'dev-box';
    patch.containerHome = '~/containers/dev-box';
    patch.packagesInstallList = ['neovim', 'ripgrep', 'git', 'fish'];
    patch.packagesRemoveList = ['vim-minimal'];
    patch.intWayland = true;
    patch.intAudio = true;
    patch.intGpu = 'auto';
    patch.intDbus = true;
    patch.intNotify = true;
    patch.intClipboard = true;
    patch.intSyncFonts = true;
    patch.intSyncIcons = true;
    patch.intSyncThemes = true;
    patch.intSshAgent = false;
    patch.intGpgAgent = false;
    patch.netMode = 'private';
    patch.portMappingsList = ['8080:80'];
    patch.secLabelDisable = true;
    patch.noNewPrivileges = true;
    patch.readOnlyRootfs = false;
    patch.usernsMode = 'keep-id';
    patch.capPreset = 'default';
    patch.lifeQuadlet = true;
    patch.lifeAutostart = false;
  return patch;
}
