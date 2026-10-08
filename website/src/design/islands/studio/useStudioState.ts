import { useState, useEffect, useRef } from 'react';
import type { MountItem, EnvVarItem, HostExecItem, SecretItem, ServiceItem } from './types';
import { STUDIO_DEFAULTS, UI_ONLY_KEYS } from './schema';

// Bump the suffix when the value shape changes; older payloads are then
// ignored instead of half-applying stale keys.
const STORAGE_KEY = 'podbox-studio-v1';

// State keys whose shape changed after sessions were already saved.
const XDG_DIR_KEYS = [
  'xdgDocuments', 'xdgDownloads', 'xdgPictures', 'xdgMusic',
  'xdgVideos', 'xdgDesktop', 'xdgProjects',
];

function loadSaved(): Record<string, any> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

export function useStudioState() {
const [isFullscreen, setIsFullscreen] = useState(STUDIO_DEFAULTS.isFullscreen);
const [activeCategory, setActiveCategory] = useState<
  'image' | 'container' | 'security' | 'network' | 'integration' | 'lifecycle' | 'dbus' | 'wayland' | 'dotfiles' | 'storage'
>(STUDIO_DEFAULTS.activeCategory);
const [activeView, setActiveView] = useState<'toml' | 'quadlet' | 'containerfile'>(STUDIO_DEFAULTS.activeView);
const [copied, setCopied] = useState(STUDIO_DEFAULTS.copied);
const [showExportMenu, setShowExportMenu] = useState(STUDIO_DEFAULTS.showExportMenu);
const [activePreset, setActivePreset] = useState<string>(STUDIO_DEFAULTS.activePreset);

// --- CONFIGURATION STATE ---

// [image]
const [imageType, setImageType] = useState<'preset' | 'custom'>(STUDIO_DEFAULTS.imageType);
const [selectedPresetDistro, setSelectedPresetDistro] = useState<string>(STUDIO_DEFAULTS.selectedPresetDistro);
const [customImageBase, setCustomImageBase] = useState<string>(STUDIO_DEFAULTS.customImageBase);
const [imageName, setImageName] = useState(STUDIO_DEFAULTS.imageName);
const [imagePrebuiltRef, setImagePrebuiltRef] = useState(STUDIO_DEFAULTS.imagePrebuiltRef);
const [pullRetry, setPullRetry] = useState(STUDIO_DEFAULTS.pullRetry);
const [pullRetryDelay, setPullRetryDelay] = useState(STUDIO_DEFAULTS.pullRetryDelay);
const [packagesInstallList, setPackagesInstallList] = useState<string[]>(STUDIO_DEFAULTS.packagesInstallList);
const [packagesRemoveList, setPackagesRemoveList] = useState<string[]>(STUDIO_DEFAULTS.packagesRemoveList);
const [packageManager, setPackageManager] = useState<string>(STUDIO_DEFAULTS.packageManager);
const [runCommands, setRunCommands] = useState<string[]>(STUDIO_DEFAULTS.runCommands);

// [dotfiles]
const [dotfilesSource, setDotfilesSource] = useState(STUDIO_DEFAULTS.dotfilesSource);
const [dotfilesTarget, setDotfilesTarget] = useState(STUDIO_DEFAULTS.dotfilesTarget);
const [dotfilesCloneOn, setDotfilesCloneOn] = useState<string>(STUDIO_DEFAULTS.dotfilesCloneOn);
const [dotfilesInstall, setDotfilesInstall] = useState(STUDIO_DEFAULTS.dotfilesInstall);

// [storage]
const [sharedCaches, setSharedCaches] = useState<string[]>(STUDIO_DEFAULTS.sharedCaches);
const [hostCaches, setHostCaches] = useState<string[]>(STUDIO_DEFAULTS.hostCaches);

// [container]
const [containerName, setContainerName] = useState(STUDIO_DEFAULTS.containerName);
const [containerHome, setContainerHome] = useState(STUDIO_DEFAULTS.containerHome);
const [containerShell, setContainerShell] = useState(STUDIO_DEFAULTS.containerShell);
const [containerMemory, setContainerMemory] = useState(STUDIO_DEFAULTS.containerMemory);
const [containerCpus, setContainerCpus] = useState(STUDIO_DEFAULTS.containerCpus);
const [containerSlice, setContainerSlice] = useState(STUDIO_DEFAULTS.containerSlice);
const [containerCpuWeight, setContainerCpuWeight] = useState(STUDIO_DEFAULTS.containerCpuWeight);
const [containerReloadCmd, setContainerReloadCmd] = useState(STUDIO_DEFAULTS.containerReloadCmd);
const [extraMounts, setExtraMounts] = useState<MountItem[]>(STUDIO_DEFAULTS.extraMounts);
const [envVars, setEnvVars] = useState<EnvVarItem[]>(STUDIO_DEFAULTS.envVars);
const [envForward, setEnvForward] = useState<string[]>(STUDIO_DEFAULTS.envForward);
const [services, setServices] = useState<ServiceItem[]>(STUDIO_DEFAULTS.services);

// [security]
const [apparmor, setApparmor] = useState(STUDIO_DEFAULTS.apparmor);
const [seccomp, setSeccomp] = useState(STUDIO_DEFAULTS.seccomp);
const [secLabelDisable, setSecLabelDisable] = useState(STUDIO_DEFAULTS.secLabelDisable);
const [noNewPrivileges, setNoNewPrivileges] = useState(STUDIO_DEFAULTS.noNewPrivileges);
const [readOnlyRootfs, setReadOnlyRootfs] = useState(STUDIO_DEFAULTS.readOnlyRootfs);
const [usernsMode, setUsernsMode] = useState<string>(STUDIO_DEFAULTS.usernsMode);
const [capPreset, setCapPreset] = useState<string>(STUDIO_DEFAULTS.capPreset);
const [extraCapAddList, setExtraCapAddList] = useState<string[]>(STUDIO_DEFAULTS.extraCapAddList);

// [network]
const [netMode, setNetMode] = useState<string>(STUDIO_DEFAULTS.netMode);
const [netOffline, setNetOffline] = useState(STUDIO_DEFAULTS.netOffline);
const [portMappingsList, setPortMappingsList] = useState<string[]>(STUDIO_DEFAULTS.portMappingsList);

// [integration]
const [intGitIdentity, setIntGitIdentity] = useState(STUDIO_DEFAULTS.intGitIdentity);
const [intWayland, setIntWayland] = useState(STUDIO_DEFAULTS.intWayland);
const [intAudio, setIntAudio] = useState(STUDIO_DEFAULTS.intAudio);
const [intGpu, setIntGpu] = useState<string>(STUDIO_DEFAULTS.intGpu);
const [intDbus, setIntDbus] = useState(STUDIO_DEFAULTS.intDbus);
const [intNotify, setIntNotify] = useState(STUDIO_DEFAULTS.intNotify);
const [intXdgOpen, setIntXdgOpen] = useState(STUDIO_DEFAULTS.intXdgOpen);
const [intClipboard, setIntClipboard] = useState(STUDIO_DEFAULTS.intClipboard);
const [intSyncFonts, setIntSyncFonts] = useState(STUDIO_DEFAULTS.intSyncFonts);
const [intSyncIcons, setIntSyncIcons] = useState(STUDIO_DEFAULTS.intSyncIcons);
const [intSyncThemes, setIntSyncThemes] = useState(STUDIO_DEFAULTS.intSyncThemes);
const [intSshAgent, setIntSshAgent] = useState(STUDIO_DEFAULTS.intSshAgent);
const [intGpgAgent, setIntGpgAgent] = useState(STUDIO_DEFAULTS.intGpgAgent);

// [integration.host_exec]
const [hostExecEnabled, setHostExecEnabled] = useState(STUDIO_DEFAULTS.hostExecEnabled);
const [hostExecList, setHostExecList] = useState<HostExecItem[]>(STUDIO_DEFAULTS.hostExecList);

// [integration.hardware]
const [hwKvm, setHwKvm] = useState(STUDIO_DEFAULTS.hwKvm);
const [hwJoystick, setHwJoystick] = useState(STUDIO_DEFAULTS.hwJoystick);
const [hwWebcam, setHwWebcam] = useState(STUDIO_DEFAULTS.hwWebcam);
const [hwSerial, setHwSerial] = useState(STUDIO_DEFAULTS.hwSerial);
const [hwYubikey, setHwYubikey] = useState(STUDIO_DEFAULTS.hwYubikey);

// [security].secrets
const [secrets, setSecrets] = useState<SecretItem[]>(STUDIO_DEFAULTS.secrets);

// [integration.xdg_dirs]
const [xdgDocuments, setXdgDocuments] = useState(STUDIO_DEFAULTS.xdgDocuments);
const [xdgDownloads, setXdgDownloads] = useState(STUDIO_DEFAULTS.xdgDownloads);
const [xdgPictures, setXdgPictures] = useState(STUDIO_DEFAULTS.xdgPictures);
const [xdgMusic, setXdgMusic] = useState(STUDIO_DEFAULTS.xdgMusic);
const [xdgVideos, setXdgVideos] = useState(STUDIO_DEFAULTS.xdgVideos);
const [xdgDesktop, setXdgDesktop] = useState(STUDIO_DEFAULTS.xdgDesktop);
const [xdgProjects, setXdgProjects] = useState(STUDIO_DEFAULTS.xdgProjects);

// [integration.export]
const [exportAppsList, setExportAppsList] = useState<string[]>(STUDIO_DEFAULTS.exportAppsList);
const [exportBinsList, setExportBinsList] = useState<string[]>(STUDIO_DEFAULTS.exportBinsList);

// [lifecycle]
const [lifeQuadlet, setLifeQuadlet] = useState(STUDIO_DEFAULTS.lifeQuadlet);
const [lifeAutostart, setLifeAutostart] = useState(STUDIO_DEFAULTS.lifeAutostart);
const [lifeOnStop, setLifeOnStop] = useState<string>(STUDIO_DEFAULTS.lifeOnStop);
const [lifeAutoUpdate, setLifeAutoUpdate] = useState(STUDIO_DEFAULTS.lifeAutoUpdate);
const [lifeAutoCheckpoint, setLifeAutoCheckpoint] = useState(STUDIO_DEFAULTS.lifeAutoCheckpoint);
const [lifeIdleTimeout, setLifeIdleTimeout] = useState<string>(STUDIO_DEFAULTS.lifeIdleTimeout);

// [systemd]
const [sysRequires, setSysRequires] = useState(STUDIO_DEFAULTS.sysRequires);
const [sysAfter, setSysAfter] = useState(STUDIO_DEFAULTS.sysAfter);

// [dbus]
const [dbusPreset, setDbusPreset] = useState<string>(STUDIO_DEFAULTS.dbusPreset);
const [dbusTalkList, setDbusTalkList] = useState<string[]>(STUDIO_DEFAULTS.dbusTalkList);
const [dbusOwnList, setDbusOwnList] = useState<string[]>(STUDIO_DEFAULTS.dbusOwnList);

// [wayland]
const [waylandFirewall, setWaylandFirewall] = useState(STUDIO_DEFAULTS.waylandFirewall);
const [waylandBlockedList, setWaylandBlockedList] = useState<string[]>(STUDIO_DEFAULTS.waylandBlockedList);


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
  const values = { isFullscreen, activeCategory, activeView, copied, showExportMenu, activePreset, imageType, selectedPresetDistro, customImageBase, imageName, imagePrebuiltRef, pullRetry, pullRetryDelay, packagesInstallList, packagesRemoveList, packageManager, runCommands, dotfilesSource, dotfilesTarget, dotfilesCloneOn, dotfilesInstall, sharedCaches, hostCaches, containerName, containerHome, containerShell, containerMemory, containerCpus, containerSlice, containerCpuWeight, containerReloadCmd, extraMounts, envVars, envForward, services, apparmor, seccomp, secLabelDisable, noNewPrivileges, readOnlyRootfs, usernsMode, capPreset, extraCapAddList, netMode, netOffline, portMappingsList, intGitIdentity, intWayland, intAudio, intGpu, intDbus, intNotify, intXdgOpen, intClipboard, intSyncFonts, intSyncIcons, intSyncThemes, intSshAgent, intGpgAgent, hostExecEnabled, hostExecList, hwKvm, hwJoystick, hwWebcam, hwSerial, hwYubikey, secrets, xdgDocuments, xdgDownloads, xdgPictures, xdgMusic, xdgVideos, xdgDesktop, xdgProjects, exportAppsList, exportBinsList, lifeQuadlet, lifeAutostart, lifeOnStop, lifeAutoUpdate, lifeAutoCheckpoint, lifeIdleTimeout, sysRequires, sysAfter, dbusPreset, dbusTalkList, dbusOwnList, waylandFirewall, waylandBlockedList };
  // Persist only configuration, never view state (a reload should not
  // re-open the fullscreen editor or a dropdown).
  const UI_ONLY = new Set<string>(UI_ONLY_KEYS);
  const persistable = Object.fromEntries(
    Object.entries(values).filter(([k]) => !UI_ONLY.has(k))
  ) as Record<string, any>;

  const restoredRef = useRef(false);
  const [hasRestoredSession, setHasRestoredSession] = useState(false);
  const lastSaved = useRef('');

  // Restore once on mount. Declared before the save effect so the
  // restored values land before anything can be written back.
  useEffect(() => {
    const saved = loadSaved();
    if (Object.keys(saved).length > 0) {
      // Sessions saved before RUN commands became a list stored them as one
      // newline-joined string; split it back so old sessions survive.
      if (typeof saved.runCommands === 'string') {
        saved.runCommands = (saved.runCommands as string).split('\n').filter((c) => c.trim());
      }
      // XDG dirs were plain booleans before the read-write mode existed.
      for (const key of XDG_DIR_KEYS) {
        if (typeof saved[key] === 'boolean') {
          saved[key] = saved[key] ? 'ro' : 'off';
        }
      }
      applyPatch(saved);
      setHasRestoredSession(true);
    }
    restoredRef.current = true;
  }, []);

  // Debounced write: the studio re-renders on every keystroke, so write
  // at most every 400ms and skip when the payload is unchanged.
  useEffect(() => {
    if (!restoredRef.current) return;
    const timer = window.setTimeout(() => {
      try {
        const json = JSON.stringify(persistable);
        if (json === lastSaved.current) return;
        lastSaved.current = json;
        localStorage.setItem(STORAGE_KEY, json);
      } catch {
        // Private mode or quota exceeded: session just won't persist.
      }
    }, 400);
    return () => window.clearTimeout(timer);
  }, [persistable]);

  const clearSavedSession = () => {
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {}
    lastSaved.current = '';
    setHasRestoredSession(false);
  };

  return { isFullscreen, activeCategory, activeView, copied, showExportMenu, activePreset, imageType, selectedPresetDistro, customImageBase, imageName, imagePrebuiltRef, pullRetry, pullRetryDelay, packagesInstallList, packagesRemoveList, packageManager, runCommands, dotfilesSource, dotfilesTarget, dotfilesCloneOn, dotfilesInstall, sharedCaches, hostCaches, containerName, containerHome, containerShell, containerMemory, containerCpus, containerSlice, containerCpuWeight, containerReloadCmd, extraMounts, envVars, envForward, services, apparmor, seccomp, secLabelDisable, noNewPrivileges, readOnlyRootfs, usernsMode, capPreset, extraCapAddList, netMode, netOffline, portMappingsList, intGitIdentity, intWayland, intAudio, intGpu, intDbus, intNotify, intXdgOpen, intClipboard, intSyncFonts, intSyncIcons, intSyncThemes, intSshAgent, intGpgAgent, hostExecEnabled, hostExecList, xdgDocuments, xdgDownloads, xdgPictures, xdgMusic, xdgVideos, xdgDesktop, xdgProjects, exportAppsList, exportBinsList, secrets, hwKvm, hwJoystick, hwWebcam, hwSerial, hwYubikey, lifeQuadlet, lifeAutostart, lifeOnStop, lifeAutoUpdate, lifeAutoCheckpoint, lifeIdleTimeout, sysRequires, sysAfter, dbusPreset, dbusTalkList, dbusOwnList, waylandFirewall, waylandBlockedList, setIsFullscreen, setActiveCategory, setActiveView, setCopied, setShowExportMenu, setActivePreset, setImageType, setSelectedPresetDistro, setCustomImageBase, setImageName, setImagePrebuiltRef, setPullRetry, setPullRetryDelay, setPackagesInstallList, setPackagesRemoveList, setPackageManager, setRunCommands, setDotfilesSource, setDotfilesTarget, setDotfilesCloneOn, setDotfilesInstall, setSharedCaches, setHostCaches, setContainerName, setContainerHome, setContainerShell, setContainerMemory, setContainerCpus, setContainerSlice, setContainerCpuWeight, setContainerReloadCmd, setExtraMounts, setEnvVars, setEnvForward, setServices, setApparmor, setSeccomp, setSecLabelDisable, setNoNewPrivileges, setReadOnlyRootfs, setUsernsMode, setCapPreset, setExtraCapAddList, setNetMode, setNetOffline, setPortMappingsList, setIntGitIdentity, setIntWayland, setIntAudio, setIntGpu, setIntDbus, setIntNotify, setIntXdgOpen, setIntClipboard, setIntSyncFonts, setIntSyncIcons, setIntSyncThemes, setIntSshAgent, setIntGpgAgent, setHostExecEnabled, setHostExecList, setHwKvm, setHwJoystick, setHwWebcam, setHwSerial, setHwYubikey, setSecrets, setXdgDocuments, setXdgDownloads, setXdgPictures, setXdgMusic, setXdgVideos, setXdgDesktop, setXdgProjects, setExportAppsList, setExportBinsList, setLifeQuadlet, setLifeAutostart, setLifeOnStop, setLifeAutoUpdate, setLifeAutoCheckpoint, setLifeIdleTimeout, setSysRequires, setSysAfter, setDbusPreset, setDbusTalkList, setDbusOwnList, setWaylandFirewall, setWaylandBlockedList, handleContainerNameChange, applyPatch, values, setters, hasRestoredSession, clearSavedSession };
}

export type StudioState = ReturnType<typeof useStudioState>;
export type StudioValues = StudioState['values'];
