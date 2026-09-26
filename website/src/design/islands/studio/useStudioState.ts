import { useState, useEffect } from 'react';
import type { MountItem, EnvVarItem, HostExecItem } from './types';

export function useStudioState() {
const [isFullscreen, setIsFullscreen] = useState(false);
const [activeCategory, setActiveCategory] = useState<
  'image' | 'container' | 'security' | 'network' | 'integration' | 'lifecycle' | 'dbus' | 'wayland'
>('image');
const [activeView, setActiveView] = useState<'toml' | 'quadlet'>('toml');
const [copied, setCopied] = useState(false);
const [showExportMenu, setShowExportMenu] = useState(false);
const [activePreset, setActivePreset] = useState<string>('custom');

// --- CONFIGURATION STATE ---

// [image]
const [imageType, setImageType] = useState<'preset' | 'custom'>('preset');
const [selectedPresetDistro, setSelectedPresetDistro] = useState<string>('fedora:44');
const [customImageBase, setCustomImageBase] = useState<string>('ghcr.io/username/custom-env:latest');
const [imageName, setImageName] = useState('dev-box');
const [imagePrebuiltRef, setImagePrebuiltRef] = useState('');
const [pullRetry, setPullRetry] = useState(3);
const [pullRetryDelay, setPullRetryDelay] = useState('5s');
const [packagesInstallList, setPackagesInstallList] = useState<string[]>([
  'neovim',
  'ripgrep',
  'git',
  'fish',
]);
const [packagesRemoveList, setPackagesRemoveList] = useState<string[]>(['vim-minimal']);
const [packageManager, setPackageManager] = useState<string>('auto');
const [runCommands, setRunCommands] = useState('dnf clean all');

// [container]
const [containerName, setContainerName] = useState('dev-box');
const [containerHome, setContainerHome] = useState('~/containers/dev-box');
const [containerShell, setContainerShell] = useState('fish');
const [containerMemory, setContainerMemory] = useState('4G');
const [containerCpus, setContainerCpus] = useState('2.0');
const [containerReloadCmd, setContainerReloadCmd] = useState('');
const [extraMounts, setExtraMounts] = useState<MountItem[]>([
  { host: '~/Projects', guest: '/home/user/Projects', mode: 'z' },
]);
const [envVars, setEnvVars] = useState<EnvVarItem[]>([
  { key: 'EDITOR', value: 'nvim' },
  { key: 'TERM', value: 'xterm-256color' },
]);

// [security]
const [apparmor, setApparmor] = useState('');
const [seccomp, setSeccomp] = useState('default');
const [secLabelDisable, setSecLabelDisable] = useState(true);
const [noNewPrivileges, setNoNewPrivileges] = useState(true);
const [readOnlyRootfs, setReadOnlyRootfs] = useState(false);
const [usernsMode, setUsernsMode] = useState<string>('keep-id');
const [capPreset, setCapPreset] = useState<string>('default');
const [extraCapAddList, setExtraCapAddList] = useState<string[]>(['SYS_PTRACE']);

// [network]
const [netMode, setNetMode] = useState<string>('private');
const [portMappingsList, setPortMappingsList] = useState<string[]>(['8080:80']);

// [integration]
const [intWayland, setIntWayland] = useState(true);
const [intAudio, setIntAudio] = useState(true);
const [intGpu, setIntGpu] = useState<string>('auto');
const [intDbus, setIntDbus] = useState(true);
const [intNotify, setIntNotify] = useState(true);
const [intXdgOpen, setIntXdgOpen] = useState(true);
const [intClipboard, setIntClipboard] = useState(true);
const [intSyncFonts, setIntSyncFonts] = useState(true);
const [intSyncIcons, setIntSyncIcons] = useState(true);
const [intSyncThemes, setIntSyncThemes] = useState(true);
const [intSshAgent, setIntSshAgent] = useState(false);
const [intGpgAgent, setIntGpgAgent] = useState(false);

// [integration.host_exec]
const [hostExecEnabled, setHostExecEnabled] = useState(false);
const [hostExecList, setHostExecList] = useState<HostExecItem[]>([
  { alias: 'git', path: '/usr/bin/git' },
]);

// [integration.xdg_dirs]
const [xdgDocuments, setXdgDocuments] = useState(false);
const [xdgDownloads, setXdgDownloads] = useState(true);
const [xdgPictures, setXdgPictures] = useState(false);
const [xdgMusic, setXdgMusic] = useState(false);
const [xdgVideos, setXdgVideos] = useState(false);
const [xdgDesktop, setXdgDesktop] = useState(false);
const [xdgProjects, setXdgProjects] = useState(true);

// [integration.export]
const [exportAppsList, setExportAppsList] = useState<string[]>(['gedit', 'nautilus']);
const [exportBinsList, setExportBinsList] = useState<string[]>(['rg', 'cargo']);

// [lifecycle]
const [lifeQuadlet, setLifeQuadlet] = useState(true);
const [lifeAutostart, setLifeAutostart] = useState(false);
const [lifeOnStop, setLifeOnStop] = useState<string>('keep');
const [lifeAutoUpdate, setLifeAutoUpdate] = useState(false);
const [lifeIdleTimeout, setLifeIdleTimeout] = useState<string>('off');

// [systemd]
const [sysRequires, setSysRequires] = useState('');
const [sysAfter, setSysAfter] = useState('network-online.target');

// [dbus]
const [dbusPreset, setDbusPreset] = useState<string>('portal');
const [dbusTalkList, setDbusTalkList] = useState<string[]>(['org.freedesktop.Notifications']);
const [dbusOwnList, setDbusOwnList] = useState<string[]>([]);

// [wayland]
const [waylandFirewall, setWaylandFirewall] = useState(true);
const [waylandBlockedList, setWaylandBlockedList] = useState<string[]>([
  'zwlr_screencopy_manager_v1',
  'ext_image_copy_capture_v1',
]);


// Keyboard shortcut for Esc when fullscreen and body scroll lock
useEffect(() => {
  if (isFullscreen) {
    document.body.style.overflow = 'hidden';
  } else {
    document.body.style.overflow = '';
  }
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && isFullscreen) {
      setIsFullscreen(false);
    }
  };
  window.addEventListener('keydown', handleKeyDown);
  return () => {
    window.removeEventListener('keydown', handleKeyDown);
    document.body.style.overflow = '';
  };
}, [isFullscreen]);

const handleContainerNameChange = (val: string) => {
  const clean = val.toLowerCase().replace(/[^a-z0-9_-]/g, '');
  setContainerName(clean);
  setImageName(clean);
  setContainerHome(`~/containers/${clean}`);
};

  const setters: Record<string, (v: any) => void> = { isFullscreen: setIsFullscreen, activeCategory: setActiveCategory, activeView: setActiveView, copied: setCopied, showExportMenu: setShowExportMenu, activePreset: setActivePreset, imageType: setImageType, selectedPresetDistro: setSelectedPresetDistro, customImageBase: setCustomImageBase, imageName: setImageName, imagePrebuiltRef: setImagePrebuiltRef, pullRetry: setPullRetry, pullRetryDelay: setPullRetryDelay, packagesInstallList: setPackagesInstallList, packagesRemoveList: setPackagesRemoveList, packageManager: setPackageManager, runCommands: setRunCommands, containerName: setContainerName, containerHome: setContainerHome, containerShell: setContainerShell, containerMemory: setContainerMemory, containerCpus: setContainerCpus, containerReloadCmd: setContainerReloadCmd, extraMounts: setExtraMounts, envVars: setEnvVars, apparmor: setApparmor, seccomp: setSeccomp, secLabelDisable: setSecLabelDisable, noNewPrivileges: setNoNewPrivileges, readOnlyRootfs: setReadOnlyRootfs, usernsMode: setUsernsMode, capPreset: setCapPreset, extraCapAddList: setExtraCapAddList, netMode: setNetMode, portMappingsList: setPortMappingsList, intWayland: setIntWayland, intAudio: setIntAudio, intGpu: setIntGpu, intDbus: setIntDbus, intNotify: setIntNotify, intXdgOpen: setIntXdgOpen, intClipboard: setIntClipboard, intSyncFonts: setIntSyncFonts, intSyncIcons: setIntSyncIcons, intSyncThemes: setIntSyncThemes, intSshAgent: setIntSshAgent, intGpgAgent: setIntGpgAgent, hostExecEnabled: setHostExecEnabled, hostExecList: setHostExecList, xdgDocuments: setXdgDocuments, xdgDownloads: setXdgDownloads, xdgPictures: setXdgPictures, xdgMusic: setXdgMusic, xdgVideos: setXdgVideos, xdgDesktop: setXdgDesktop, xdgProjects: setXdgProjects, exportAppsList: setExportAppsList, exportBinsList: setExportBinsList, lifeQuadlet: setLifeQuadlet, lifeAutostart: setLifeAutostart, lifeOnStop: setLifeOnStop, lifeAutoUpdate: setLifeAutoUpdate, lifeIdleTimeout: setLifeIdleTimeout, sysRequires: setSysRequires, sysAfter: setSysAfter, dbusPreset: setDbusPreset, dbusTalkList: setDbusTalkList, dbusOwnList: setDbusOwnList, waylandFirewall: setWaylandFirewall, waylandBlockedList: setWaylandBlockedList };
  const applyPatch = (patch: Record<string, any>) => {
    for (const [k, v] of Object.entries(patch)) setters[k]?.(v as never);
  };
  const values = { isFullscreen, activeCategory, activeView, copied, showExportMenu, activePreset, imageType, selectedPresetDistro, customImageBase, imageName, imagePrebuiltRef, pullRetry, pullRetryDelay, packagesInstallList, packagesRemoveList, packageManager, runCommands, containerName, containerHome, containerShell, containerMemory, containerCpus, containerReloadCmd, extraMounts, envVars, apparmor, seccomp, secLabelDisable, noNewPrivileges, readOnlyRootfs, usernsMode, capPreset, extraCapAddList, netMode, portMappingsList, intWayland, intAudio, intGpu, intDbus, intNotify, intXdgOpen, intClipboard, intSyncFonts, intSyncIcons, intSyncThemes, intSshAgent, intGpgAgent, hostExecEnabled, hostExecList, xdgDocuments, xdgDownloads, xdgPictures, xdgMusic, xdgVideos, xdgDesktop, xdgProjects, exportAppsList, exportBinsList, lifeQuadlet, lifeAutostart, lifeOnStop, lifeAutoUpdate, lifeIdleTimeout, sysRequires, sysAfter, dbusPreset, dbusTalkList, dbusOwnList, waylandFirewall, waylandBlockedList };
  return { isFullscreen, activeCategory, activeView, copied, showExportMenu, activePreset, imageType, selectedPresetDistro, customImageBase, imageName, imagePrebuiltRef, pullRetry, pullRetryDelay, packagesInstallList, packagesRemoveList, packageManager, runCommands, containerName, containerHome, containerShell, containerMemory, containerCpus, containerReloadCmd, extraMounts, envVars, apparmor, seccomp, secLabelDisable, noNewPrivileges, readOnlyRootfs, usernsMode, capPreset, extraCapAddList, netMode, portMappingsList, intWayland, intAudio, intGpu, intDbus, intNotify, intXdgOpen, intClipboard, intSyncFonts, intSyncIcons, intSyncThemes, intSshAgent, intGpgAgent, hostExecEnabled, hostExecList, xdgDocuments, xdgDownloads, xdgPictures, xdgMusic, xdgVideos, xdgDesktop, xdgProjects, exportAppsList, exportBinsList, lifeQuadlet, lifeAutostart, lifeOnStop, lifeAutoUpdate, lifeIdleTimeout, sysRequires, sysAfter, dbusPreset, dbusTalkList, dbusOwnList, waylandFirewall, waylandBlockedList, setIsFullscreen, setActiveCategory, setActiveView, setCopied, setShowExportMenu, setActivePreset, setImageType, setSelectedPresetDistro, setCustomImageBase, setImageName, setImagePrebuiltRef, setPullRetry, setPullRetryDelay, setPackagesInstallList, setPackagesRemoveList, setPackageManager, setRunCommands, setContainerName, setContainerHome, setContainerShell, setContainerMemory, setContainerCpus, setContainerReloadCmd, setExtraMounts, setEnvVars, setApparmor, setSeccomp, setSecLabelDisable, setNoNewPrivileges, setReadOnlyRootfs, setUsernsMode, setCapPreset, setExtraCapAddList, setNetMode, setPortMappingsList, setIntWayland, setIntAudio, setIntGpu, setIntDbus, setIntNotify, setIntXdgOpen, setIntClipboard, setIntSyncFonts, setIntSyncIcons, setIntSyncThemes, setIntSshAgent, setIntGpgAgent, setHostExecEnabled, setHostExecList, setXdgDocuments, setXdgDownloads, setXdgPictures, setXdgMusic, setXdgVideos, setXdgDesktop, setXdgProjects, setExportAppsList, setExportBinsList, setLifeQuadlet, setLifeAutostart, setLifeOnStop, setLifeAutoUpdate, setLifeIdleTimeout, setSysRequires, setSysAfter, setDbusPreset, setDbusTalkList, setDbusOwnList, setWaylandFirewall, setWaylandBlockedList, handleContainerNameChange, applyPatch, values, setters };
}

export type StudioState = ReturnType<typeof useStudioState>;
export type StudioValues = StudioState['values'];
