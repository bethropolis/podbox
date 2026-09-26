import React, { useState, useEffect } from 'react';
import { PodboxLogo } from '../components/PodboxLogo';
import { StudioTooltip } from '../components/StudioTooltip';
import {
  StudioSwitch,
  StudioInput,
  StudioSelect,
  StudioTagInput,
} from '../components/StudioControls';
import {
  Box,
  Cpu,
  Layers3,
  ShieldCheck,
  Network,
  Monitor,
  Volume2,
  RefreshCw,
  Radio,
  Download,
  Copy,
  Check,
  Maximize2,
  Minimize2,
  RotateCcw,
  Sparkles,
  ArrowLeft,
  ChevronDown,
  Terminal,
  ExternalLink,
  Plus,
  Trash2,
  CheckCircle2,
  FileCode2,
  SlidersHorizontal,
  Flame,
  Globe,
  Lock,
  Boxes
} from 'lucide-react';

interface MountItem {
  host: string;
  guest: string;
  mode: string;
}

interface EnvVarItem {
  key: string;
  value: string;
}

interface HostExecItem {
  alias: string;
  path: string;
}

import { withBase } from '../base';

interface StudioPageProps {
  // Navigation is plain MPA links; no callback props cross the Astro boundary.
}

export function StudioPage(_props: StudioPageProps) {
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

  // Presets
  const applyPreset = (preset: 'rust' | 'arch-gui' | 'fullstack' | 'minimal') => {
    setActivePreset(preset);
    if (preset === 'rust') {
      setImageType('preset');
      setSelectedPresetDistro('fedora:44');
      setContainerName('rust-dev');
      setImageName('rust-dev');
      setContainerHome('~/containers/rust-dev');
      setPackagesInstallList(['cargo', 'rustc', 'neovim', 'ripgrep', 'git', 'mold']);
      setIntWayland(true);
      setIntAudio(true);
      setIntGpu('auto');
      setIntSshAgent(true);
      setNetMode('private');
      setLifeQuadlet(true);
    } else if (preset === 'arch-gui') {
      setImageType('preset');
      setSelectedPresetDistro('archlinux:latest');
      setContainerName('arch-desktop');
      setImageName('arch-desktop');
      setContainerHome('~/containers/arch-desktop');
      setPackagesInstallList(['firefox', 'alacritty', 'neovim', 'mesa', 'pipewire']);
      setIntWayland(true);
      setIntAudio(true);
      setIntGpu('auto');
      setIntClipboard(true);
      setDbusPreset('portal');
      setLifeQuadlet(true);
    } else if (preset === 'fullstack') {
      setImageType('preset');
      setSelectedPresetDistro('ubuntu:24.04');
      setContainerName('fullstack-web');
      setImageName('fullstack-web');
      setContainerHome('~/containers/fullstack-web');
      setPackagesInstallList(['nodejs', 'npm', 'pnpm', 'git', 'curl', 'python3']);
      setPortMappingsList(['3000:3000', '5173:5173', '8080:8080']);
      setNetMode('pasta');
      setIntWayland(false);
      setIntAudio(false);
      setIntGpu('false');
      setLifeAutostart(true);
    } else if (preset === 'minimal') {
      setImageType('preset');
      setSelectedPresetDistro('alpine:3.20');
      setContainerName('micro-box');
      setImageName('micro-box');
      setContainerHome('~/containers/micro-box');
      setPackagesInstallList(['busybox-extras', 'curl', 'ca-certificates']);
      setIntWayland(false);
      setIntAudio(false);
      setIntGpu('false');
      setIntDbus(false);
      setReadOnlyRootfs(true);
      setNoNewPrivileges(true);
      setNetMode('none');
    }
  };

  const resetToDefault = () => {
    setActivePreset('custom');
    setImageType('preset');
    setSelectedPresetDistro('fedora:44');
    setContainerName('dev-box');
    setImageName('dev-box');
    setContainerHome('~/containers/dev-box');
    setPackagesInstallList(['neovim', 'ripgrep', 'git', 'fish']);
    setPackagesRemoveList(['vim-minimal']);
    setIntWayland(true);
    setIntAudio(true);
    setIntGpu('auto');
    setIntDbus(true);
    setIntNotify(true);
    setIntClipboard(true);
    setIntSyncFonts(true);
    setIntSyncIcons(true);
    setIntSyncThemes(true);
    setIntSshAgent(false);
    setIntGpgAgent(false);
    setNetMode('private');
    setPortMappingsList(['8080:80']);
    setSecLabelDisable(true);
    setNoNewPrivileges(true);
    setReadOnlyRootfs(false);
    setUsernsMode('keep-id');
    setCapPreset('default');
    setLifeQuadlet(true);
    setLifeAutostart(false);
  };

  // Generate podbox.toml
  const generateFullToml = () => {
    let t = `# podbox.toml — generated by podbox Studio\n\n`;

    // [image]
    t += `[image]\n`;
    if (imageType === 'preset') {
      t += `preset = "${selectedPresetDistro}"\n`;
    } else {
      t += `base = "${customImageBase}"\n`;
    }
    if (imageName && imageName !== containerName) {
      t += `name = "${imageName}"\n`;
    }
    if (imagePrebuiltRef) {
      t += `prebuilt = "${imagePrebuiltRef}"\n`;
    }
    if (pullRetry !== 3) t += `pull_retry = ${pullRetry}\n`;
    if (pullRetryDelay !== '5s') t += `pull_retry_delay = "${pullRetryDelay}"\n`;
    if (packageManager !== 'auto') t += `package_manager = "${packageManager}"\n`;

    if (packagesInstallList.length > 0) {
      t += `packages = [${packagesInstallList.map((p) => `"${p}"`).join(', ')}]\n`;
    }
    if (packagesRemoveList.length > 0) {
      t += `remove_packages = [${packagesRemoveList.map((p) => `"${p}"`).join(', ')}]\n`;
    }
    if (runCommands.trim()) {
      t += `run = [\n${runCommands
        .split('\n')
        .filter((r) => r.trim())
        .map((r) => `  "${r.trim()}",\n`)
        .join('')}]\n`;
    }

    // [container]
    t += `\n[container]\n`;
    t += `name = "${containerName}"\n`;
    if (containerHome) t += `home = "${containerHome}"\n`;
    if (containerShell) t += `shell = "${containerShell}"\n`;
    if (containerMemory) t += `memory = "${containerMemory}"\n`;
    if (containerCpus) t += `cpus = "${containerCpus}"\n`;
    if (containerReloadCmd) t += `reload_cmd = "${containerReloadCmd}"\n`;

    const validMounts = extraMounts.filter((m) => m.host.trim() && m.guest.trim());
    if (validMounts.length > 0) {
      t += `mounts = [\n${validMounts
        .map((m) => `  "${m.host}:${m.guest}:${m.mode || 'z'}",\n`)
        .join('')}]\n`;
    }

    const validEnvs = envVars.filter((e) => e.key.trim());
    if (validEnvs.length > 0) {
      t += `env = {\n${validEnvs
        .map((e) => `  ${e.key} = "${e.value}",\n`)
        .join('')}}\n`;
    }

    // [security]
    const hasSecurity =
      apparmor ||
      seccomp !== 'default' ||
      !secLabelDisable ||
      !noNewPrivileges ||
      readOnlyRootfs ||
      usernsMode !== 'keep-id' ||
      capPreset !== 'default' ||
      extraCapAddList.length > 0;
    if (hasSecurity) {
      t += `\n[security]\n`;
      if (apparmor) t += `apparmor = "${apparmor}"\n`;
      if (seccomp !== 'default') t += `seccomp = "${seccomp}"\n`;
      if (!secLabelDisable) t += `security_label_disable = false\n`;
      if (!noNewPrivileges) t += `no_new_privileges = false\n`;
      if (readOnlyRootfs) t += `read_only_rootfs = true\n`;
      if (usernsMode !== 'keep-id') t += `userns = "${usernsMode}"\n`;
      if (capPreset !== 'default') t += `cap_preset = "${capPreset}"\n`;
      if (extraCapAddList.length > 0) {
        t += `cap_add = [${extraCapAddList.map((c) => `"${c}"`).join(', ')}]\n`;
      }
    }

    // [network]
    const hasNet = netMode !== 'private' || portMappingsList.length > 0;
    if (hasNet) {
      t += `\n[network]\n`;
      t += `mode = "${netMode}"\n`;
      if (portMappingsList.length > 0 && netMode !== 'host') {
        t += `ports = [${portMappingsList.map((p) => `"${p}"`).join(', ')}]\n`;
      }
    }

    // [integration]
    t += `\n[integration]\n`;
    t += `wayland = ${intWayland}\n`;
    t += `audio = ${intAudio}\n`;
    t += `gpu = ${intGpu === 'true' || intGpu === 'false' ? intGpu : `"${intGpu}"`}\n`;
    t += `dbus = ${intDbus}\n`;
    if (!intNotify) t += `notify = false\n`;
    if (!intXdgOpen) t += `xdg_open = false\n`;
    if (!intClipboard) t += `clipboard = false\n`;
    if (!intSyncFonts) t += `sync_fonts = false\n`;
    if (!intSyncIcons) t += `sync_icons = false\n`;
    if (!intSyncThemes) t += `sync_themes = false\n`;
    if (intSshAgent) t += `ssh_agent = true\n`;
    if (intGpgAgent) t += `gpg_agent = true\n`;

    if (hostExecEnabled) {
      t += `\n[integration.host_exec]\n`;
      t += `enabled = true\n`;
      const validExecs = hostExecList.filter((e) => e.alias.trim() && e.path.trim());
      if (validExecs.length > 0) {
        t += `allowlist = { ${validExecs.map((e) => `${e.alias} = "${e.path}"`).join(', ')} }\n`;
      }
    }

    const hasXdgDirs =
      xdgDocuments || xdgDownloads || xdgPictures || xdgMusic || xdgVideos || xdgDesktop || xdgProjects;
    if (hasXdgDirs) {
      t += `\n[integration.xdg_dirs]\n`;
      if (xdgDocuments) t += `documents = true\n`;
      if (xdgDownloads) t += `downloads = true\n`;
      if (xdgPictures) t += `pictures = true\n`;
      if (xdgMusic) t += `music = true\n`;
      if (xdgVideos) t += `videos = true\n`;
      if (xdgDesktop) t += `desktop = true\n`;
      if (xdgProjects) t += `projects = true\n`;
    }

    if (exportAppsList.length > 0 || exportBinsList.length > 0) {
      t += `\n[integration.export]\n`;
      if (exportAppsList.length > 0) t += `apps = [${exportAppsList.map((a) => `"${a}"`).join(', ')}]\n`;
      if (exportBinsList.length > 0) t += `bins = [${exportBinsList.map((b) => `"${b}"`).join(', ')}]\n`;
    }

    // [lifecycle]
    const hasLifecycle =
      !lifeQuadlet || lifeAutostart || lifeOnStop !== 'keep' || lifeAutoUpdate || lifeIdleTimeout !== 'off';
    if (hasLifecycle) {
      t += `\n[lifecycle]\n`;
      if (!lifeQuadlet) t += `quadlet = false\n`;
      if (lifeAutostart) t += `autostart = true\n`;
      if (lifeOnStop !== 'keep') t += `on_stop = "${lifeOnStop}"\n`;
      if (lifeAutoUpdate) t += `auto_update = true\n`;
      if (lifeIdleTimeout !== 'off') t += `idle_timeout = "${lifeIdleTimeout}"\n`;
    }

    // [systemd]
    const reqList = sysRequires.split(',').map((s) => s.trim()).filter(Boolean);
    const afterList = sysAfter.split(',').map((s) => s.trim()).filter(Boolean);
    if (reqList.length > 0 || sysAfter !== 'network-online.target') {
      t += `\n[systemd]\n`;
      if (reqList.length > 0) t += `requires = [${reqList.map((r) => `"${r}"`).join(', ')}]\n`;
      if (afterList.length > 0) t += `after = [${afterList.map((a) => `"${a}"`).join(', ')}]\n`;
    }

    // [dbus]
    if (intDbus) {
      if (dbusPreset !== 'portal' || dbusTalkList.length > 0 || dbusOwnList.length > 0) {
        t += `\n[dbus]\n`;
        if (dbusPreset !== 'none') t += `preset = "${dbusPreset}"\n`;
        if (dbusTalkList.length > 0) t += `talk = [${dbusTalkList.map((tk) => `"${tk}"`).join(', ')}]\n`;
        if (dbusOwnList.length > 0) t += `own = [${dbusOwnList.map((o) => `"${o}"`).join(', ')}]\n`;
      }
    }

    // [wayland]
    if (intWayland && (!waylandFirewall || waylandBlockedList.length > 0)) {
      t += `\n[wayland]\n`;
      if (!waylandFirewall) t += `firewall = false\n`;
      if (waylandBlockedList.length > 0) {
        t += `blocked_interfaces = [\n${waylandBlockedList.map((b) => `  "${b}",\n`).join('')}]\n`;
      }
    }

    return t;
  };

  // Generate Quadlet
  const generateQuadlet = () => {
    let q = `# ~/.config/containers/systemd/${containerName}.container\n`;
    q += `[Unit]\n`;
    q += `Description=podbox ${containerName} container\n`;
    const afterList = sysAfter.split(',').map((s) => s.trim()).filter(Boolean);
    if (afterList.length > 0) {
      q += `After=${afterList.join(' ')}\n`;
    }
    const reqList = sysRequires.split(',').map((s) => s.trim()).filter(Boolean);
    if (reqList.length > 0) {
      q += `Requires=${reqList.join(' ')}\n`;
    }

    q += `\n[Container]\n`;
    q += `Image=localhost/podbox-${imageName || containerName}:latest\n`;
    q += `ContainerName=podbox-${containerName}\n`;
    q += `Volume=podbox-${containerName}-home:/home/user:Z\n`;

    if (containerHome.startsWith('~/')) {
      const sub = containerHome.replace(/^~\/?/, '');
      q += `Volume=%h/${sub}:/home/user:rslave,z\n`;
    }

    extraMounts.forEach((m) => {
      if (m.host && m.guest) {
        const expandedHost = m.host.replace(/^~\//, '%h/');
        q += `Volume=${expandedHost}:${m.guest}:${m.mode || 'z'}\n`;
      }
    });

    if (intWayland) {
      q += `Environment=WAYLAND_DISPLAY=wayland-0\n`;
      q += `Volume=/run/user/%U/wayland-0:/run/user/1000/wayland-0:ro\n`;
    }
    if (intAudio) {
      q += `Volume=/run/user/%U/pulse:/run/user/1000/pulse:ro\n`;
      q += `Volume=/run/user/%U/pipewire-0:/run/user/1000/pipewire-0:ro\n`;
    }
    if (intGpu === 'auto' || intGpu === 'true' || intGpu === 'nvidia') {
      q += `Device=/dev/dri\n`;
      if (intGpu === 'nvidia') {
        q += `AddDevice=nvidia.com/gpu=all\n`;
      }
    }
    if (intDbus) {
      q += `Volume=/run/user/%U/podbox-${containerName}-dbus/bus:/run/user/1000/bus:ro\n`;
    }

    if (netMode !== 'private') {
      q += `Network=${netMode}\n`;
    }
    if (netMode !== 'host') {
      portMappingsList.forEach((p) => {
        q += `PublishPort=${p}\n`;
      });
    }

    if (containerMemory) q += `Memory=${containerMemory}\n`;
    if (containerCpus) q += `CpuQuota=${Math.round(parseFloat(containerCpus) * 100)}%\n`;
    if (containerReloadCmd) q += `ReloadCmd=${containerReloadCmd}\n`;

    if (secLabelDisable) q += `SecurityLabelDisable=true\n`;
    if (noNewPrivileges) q += `NoNewPrivileges=true\n`;
    if (readOnlyRootfs) q += `ReadOnly=true\n`;
    if (usernsMode) q += `UserNS=${usernsMode}\n`;
    if (apparmor) q += `AppArmor=${apparmor}\n`;
    if (seccomp && seccomp !== 'default') q += `SeccompProfile=${seccomp}\n`;

    if (lifeAutoUpdate) q += `Label=io.containers.autoupdate=registry\n`;

    envVars.forEach((e) => {
      if (e.key) q += `Environment=${e.key}=${e.value}\n`;
    });

    q += `\n[Service]\n`;
    q += `Restart=${lifeOnStop === 'remove' ? 'no' : 'on-failure'}\n`;
    q += `TimeoutStopSec=30\n\n`;

    q += `[Install]\n`;
    q += `WantedBy=${lifeAutostart ? 'default.target' : 'multi-user.target'}\n`;

    return q;
  };

  const handleDownloadToml = () => {
    const tomlContent = generateFullToml();
    const blob = new Blob([tomlContent], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `${containerName || 'podbox'}.toml`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    setShowExportMenu(false);
  };

  const handleDownloadQuadlet = () => {
    const quadletContent = generateQuadlet();
    const blob = new Blob([quadletContent], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `${containerName || 'podbox'}.container`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    setShowExportMenu(false);
  };

  const handleCopyConfig = async () => {
    const content = activeView === 'toml' ? generateFullToml() : generateQuadlet();
    await navigator.clipboard.writeText(content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const categories = [
    { id: 'image', label: '[image]', icon: Box, count: 'Distro & Packages' },
    { id: 'container', label: '[container]', icon: Boxes, count: 'Resources & Shell' },
    { id: 'security', label: '[security]', icon: ShieldCheck, count: 'UserNS & Caps' },
    { id: 'network', label: '[network]', icon: Network, count: 'Pasta & Ports' },
    { id: 'integration', label: '[integration]', icon: Monitor, count: 'Wayland & GPU' },
    { id: 'lifecycle', label: '[lifecycle]', icon: RefreshCw, count: 'Quadlet & Boot' },
    { id: 'dbus', label: '[dbus]', icon: Radio, count: 'Proxy Rules' },
    { id: 'wayland', label: '[wayland]', icon: Layers3, count: 'Filter Protocol' },
  ];

  const currentCode = activeView === 'toml' ? generateFullToml() : generateQuadlet();
  const codeLines = currentCode.split('\n');

  const highlightCodeLine = (line: string, lang: string) => {
    const trimmed = line.trim();
    if (trimmed.startsWith('#') || trimmed.startsWith('//') || trimmed.startsWith(';')) {
      return <span className="syntax-comment">{line}</span>;
    }
    if (lang === 'toml' || lang === 'ini') {
      if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
        return <span className="syntax-section font-bold text-[var(--accent-mauve)]">{line}</span>;
      }
      const eqIdx = line.indexOf('=');
      if (eqIdx !== -1) {
        const key = line.slice(0, eqIdx);
        const val = line.slice(eqIdx + 1);
        return (
          <>
            <span className="syntax-variable text-[var(--accent-blue)]">{key}</span>
            <span className="syntax-operator text-[var(--text-muted)]">=</span>
            <span className={val.includes('"') ? 'syntax-string text-[var(--accent-green)]' : 'syntax-number text-[var(--accent-peach)]'}>
              {val}
            </span>
          </>
        );
      }
    }
    return <span>{line}</span>;
  };

  return (
    <div
      className={`w-full font-sans text-[var(--text-primary)] transition-all ${
        isFullscreen
          ? 'fixed inset-0 z-50 bg-[var(--bg-base)] flex flex-col p-3 sm:p-4 h-screen max-h-screen overflow-hidden'
          : 'max-w-7xl mx-auto px-4 sm:px-6 py-4 sm:py-6 lg:h-[calc(100vh-4.5rem)] lg:flex lg:flex-col lg:overflow-hidden'
      }`}
    >
      {/* -------------------------------------------------------------------- */}
      {/* 1. TOP HEADER & STUDIO ACTIONS BAR                                   */}
      {/* -------------------------------------------------------------------- */}
      <div className={`shrink-0 border-b border-[var(--border)] ${isFullscreen ? 'pb-2.5 mb-2.5 space-y-2' : 'pb-4 mb-4 space-y-3'}`}>
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
          {/* Left: Branding & status */}
          <div className="flex items-center gap-3">
            {!isFullscreen && (
              <a
                href={withBase('/')}
                className="p-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] rounded-[2px] transition-colors cursor-pointer"
                title="Back to Overview"
              >
                <ArrowLeft className="w-4 h-4" />
              </a>
            )}
            <PodboxLogo className="w-8 h-8 shrink-0" />
            <div>
              <div className="flex items-center gap-2">
                <h1 className="text-xl sm:text-2xl font-bold tracking-tight text-[var(--text-primary)]">
                  podbox Studio
                </h1>
                <span className="text-[10px] font-mono px-2 py-0.5 rounded-[2px] bg-[var(--accent-mauve)]/10 text-[var(--accent-mauve)] border border-[var(--accent-mauve)]/30 font-semibold uppercase">
                  Workbench
                </span>
              </div>
              <p className="text-xs text-[var(--text-muted)] mt-0.5 font-sans">
                Full-spectrum declarative container architect & systemd Quadlet synthesizer
              </p>
            </div>
          </div>

          {/* Right: Actions (Presets, Export, Reset, Fullscreen) */}
          <div className="flex flex-wrap items-center gap-2">
            {/* Status indicator */}
            <div className="hidden lg:flex items-center gap-1.5 px-2.5 py-1.5 rounded-[2px] bg-[var(--bg-crust)] border border-[var(--border)] text-[11px] font-mono text-[var(--accent-green)]">
              <span className="w-2 h-2 rounded-full bg-[var(--accent-green)] animate-pulse" />
              <span>Synthesizer Active</span>
            </div>

            {/* Reset button */}
            <button
              onClick={resetToDefault}
              type="button"
              className="px-2.5 py-1.5 rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-xs text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:border-[var(--accent-red)]/50 transition-colors cursor-pointer flex items-center gap-1.5"
              title="Reset all settings to default"
            >
              <RotateCcw className="w-3.5 h-3.5 text-[var(--text-muted)]" />
              <span className="hidden sm:inline">Reset</span>
            </button>

            {/* Export Dropdown */}
            <div className="relative">
              <button
                onClick={() => setShowExportMenu(!showExportMenu)}
                type="button"
                className="px-3 py-1.5 rounded-[2px] bg-[var(--accent-mauve)] text-[var(--bg-crust)] font-semibold text-xs transition-colors cursor-pointer flex items-center gap-1.5 shadow-sm hover:opacity-95"
              >
                <Download className="w-3.5 h-3.5" />
                <span>Export</span>
                <ChevronDown className="w-3.5 h-3.5 ml-0.5" />
              </button>

              {showExportMenu && (
                <>
                  <div
                    className="fixed inset-0 z-40"
                    onClick={() => setShowExportMenu(false)}
                  />
                  <div className="absolute right-0 top-full mt-1.5 z-50 w-60 rounded-[3px] bg-[var(--bg-mantle)] border border-[var(--border)] shadow-2xl p-1 text-xs font-sans animate-fadeIn">
                    <button
                      onClick={handleDownloadToml}
                      className="w-full text-left px-3 py-2 rounded-[2px] hover:bg-[var(--bg-surface0)] text-[var(--text-primary)] flex items-center justify-between cursor-pointer"
                    >
                      <div className="flex items-center gap-2">
                        <FileCode2 className="w-4 h-4 text-[var(--accent-mauve)]" />
                        <div>
                          <div className="font-medium">Download podbox.toml</div>
                          <div className="text-[10px] text-[var(--text-muted)] font-mono">
                            {containerName || 'podbox'}.toml
                          </div>
                        </div>
                      </div>
                    </button>

                    <button
                      onClick={handleDownloadQuadlet}
                      className="w-full text-left px-3 py-2 rounded-[2px] hover:bg-[var(--bg-surface0)] text-[var(--text-primary)] flex items-center justify-between cursor-pointer"
                    >
                      <div className="flex items-center gap-2">
                        <Terminal className="w-4 h-4 text-[var(--accent-peach)]" />
                        <div>
                          <div className="font-medium">Download Quadlet Unit</div>
                          <div className="text-[10px] text-[var(--text-muted)] font-mono">
                            {containerName || 'podbox'}.container
                          </div>
                        </div>
                      </div>
                    </button>

                    <div className="my-1 border-t border-[var(--border)]" />

                    <button
                      onClick={() => {
                        handleCopyConfig();
                        setShowExportMenu(false);
                      }}
                      className="w-full text-left px-3 py-2 rounded-[2px] hover:bg-[var(--bg-surface0)] text-[var(--text-subtext)] hover:text-[var(--text-primary)] flex items-center gap-2 cursor-pointer"
                    >
                      <Copy className="w-4 h-4 text-[var(--accent-blue)]" />
                      <span>Copy Current Output</span>
                    </button>
                  </div>
                </>
              )}
            </div>

            {/* Fullscreen toggle */}
            <button
              onClick={() => setIsFullscreen(!isFullscreen)}
              type="button"
              className="px-2.5 py-1.5 rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-xs text-[var(--text-subtext)] hover:text-[var(--text-primary)] transition-colors cursor-pointer flex items-center gap-1.5"
              title={isFullscreen ? 'Exit Fullscreen (Esc)' : 'Expand to Fullscreen'}
            >
              {isFullscreen ? (
                <>
                  <Minimize2 className="w-3.5 h-3.5 text-[var(--accent-mauve)]" />
                  <span className="hidden sm:inline">Exit Fullscreen</span>
                </>
              ) : (
                <>
                  <Maximize2 className="w-3.5 h-3.5 text-[var(--text-muted)]" />
                  <span className="hidden sm:inline">Fullscreen</span>
                </>
              )}
            </button>
          </div>
        </div>

        {/* ------------------------------------------------------------------ */}
        {/* Presets Bar                                                        */}
        {/* ------------------------------------------------------------------ */}
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2.5 pt-2">
          <div className="text-xs text-[var(--text-muted)] flex items-center gap-1.5">
            <Sparkles className="w-3.5 h-3.5 text-[var(--accent-yellow)]" />
            <span className="font-medium">Curated Presets:</span>
          </div>

          <div className="grid grid-cols-2 sm:flex sm:flex-wrap items-center gap-1.5">
            <button
              onClick={() => applyPreset('rust')}
              className={`px-2.5 py-1 rounded-[2px] text-xs font-mono transition-all flex items-center gap-1.5 cursor-pointer border ${
                activePreset === 'rust'
                  ? 'bg-[var(--accent-mauve)]/15 border-[var(--accent-mauve)] text-[var(--accent-mauve)] font-bold'
                  : 'bg-[var(--bg-mantle)] border-[var(--border)] text-[var(--text-subtext)] hover:border-[var(--border-focus)]'
              }`}
            >
              <Flame className="w-3 h-3 text-[var(--accent-peach)]" />
              <span>Rust Dev</span>
            </button>

            <button
              onClick={() => applyPreset('arch-gui')}
              className={`px-2.5 py-1 rounded-[2px] text-xs font-mono transition-all flex items-center gap-1.5 cursor-pointer border ${
                activePreset === 'arch-gui'
                  ? 'bg-[var(--accent-mauve)]/15 border-[var(--accent-mauve)] text-[var(--accent-mauve)] font-bold'
                  : 'bg-[var(--bg-mantle)] border-[var(--border)] text-[var(--text-subtext)] hover:border-[var(--border-focus)]'
              }`}
            >
              <Monitor className="w-3 h-3 text-[var(--accent-blue)]" />
              <span>Arch GUI</span>
            </button>

            <button
              onClick={() => applyPreset('fullstack')}
              className={`px-2.5 py-1 rounded-[2px] text-xs font-mono transition-all flex items-center gap-1.5 cursor-pointer border ${
                activePreset === 'fullstack'
                  ? 'bg-[var(--accent-mauve)]/15 border-[var(--accent-mauve)] text-[var(--accent-mauve)] font-bold'
                  : 'bg-[var(--bg-mantle)] border-[var(--border)] text-[var(--text-subtext)] hover:border-[var(--border-focus)]'
              }`}
            >
              <Globe className="w-3 h-3 text-[var(--accent-green)]" />
              <span>Full-Stack</span>
            </button>

            <button
              onClick={() => applyPreset('minimal')}
              className={`px-2.5 py-1 rounded-[2px] text-xs font-mono transition-all flex items-center gap-1.5 cursor-pointer border ${
                activePreset === 'minimal'
                  ? 'bg-[var(--accent-mauve)]/15 border-[var(--accent-mauve)] text-[var(--accent-mauve)] font-bold'
                  : 'bg-[var(--bg-mantle)] border-[var(--border)] text-[var(--text-subtext)] hover:border-[var(--border-focus)]'
              }`}
            >
              <Lock className="w-3 h-3 text-[var(--accent-red)]" />
              <span>Hardened</span>
            </button>
          </div>
        </div>
      </div>

      {/* -------------------------------------------------------------------- */}
      {/* 2. MAIN WORKSPACE (SETTINGS + LIVE PREVIEW)                           */}
      {/* -------------------------------------------------------------------- */}
      <div className={`grid grid-cols-1 lg:grid-cols-12 gap-4 lg:items-stretch ${isFullscreen ? 'flex-1 min-h-0 overflow-hidden' : 'lg:flex-1 lg:min-h-0 lg:overflow-hidden'}`}>
        {/* LEFT COLUMN: Categories & Settings Form (7 cols) */}
        <div className="lg:col-span-7 flex flex-col min-h-0 h-full overflow-hidden rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)]/40 shadow-sm">
          {/* Category Tabs Header */}
          <div className="shrink-0 bg-[var(--bg-crust)] border-b border-[var(--border)] px-2.5 py-1.5 flex items-center gap-1 overflow-x-auto scrollbar-none">
            {categories.map((cat) => {
              const Icon = cat.icon;
              const isActive = activeCategory === cat.id;
              return (
                <button
                  key={cat.id}
                  onClick={() => setActiveCategory(cat.id as any)}
                  type="button"
                  className={`px-3 py-1.5 rounded-[2px] text-xs font-mono transition-all flex items-center gap-1.5 whitespace-nowrap cursor-pointer shrink-0 ${
                    isActive
                      ? 'bg-[var(--bg-surface0)] text-[var(--accent-mauve)] font-bold shadow-xs'
                      : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]/50'
                  }`}
                >
                  <Icon className="w-3.5 h-3.5" />
                  <span>{cat.label}</span>
                </button>
              );
            })}
          </div>

          {/* Form Content Body - Scrollable */}
          <div className="flex-1 min-h-0 overflow-y-auto p-4 sm:p-5 space-y-6 custom-scrollbar bg-[var(--bg-base)]">
            {/* -------------------------------------------------------------- */}
            {/* [IMAGE] SECTION                                                */}
            {/* -------------------------------------------------------------- */}
            {activeCategory === 'image' && (
              <div className="space-y-5 animate-fadeIn">
                <div className="pb-3 border-b border-[var(--border)]">
                  <div className="flex items-center gap-2">
                    <Box className="w-4 h-4 text-[var(--accent-mauve)]" />
                    <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
                      [image] — Base Distribution &amp; Packages
                    </h2>
                  </div>
                  <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
                    Define the upstream base OS or OCI image, baked packages, and Containerfile RUN steps.
                  </p>
                </div>

                {/* Base Image Selection Type */}
                <div className="space-y-2">
                  <div className="flex items-center">
                    <span className="text-xs font-medium text-[var(--text-subtext)]">Base OS Source</span>
                    <StudioTooltip
                      section="[image]"
                      title="preset vs base"
                      description="Choose from curated tested distributions (Fedora, Ubuntu, Arch, Alpine, Debian) or specify any OCI image URI from ghcr.io or docker.io."
                      quadlet="FROM <base-image>"
                    />
                  </div>
                  <div className="grid grid-cols-2 gap-2">
                    <button
                      type="button"
                      onClick={() => setImageType('preset')}
                      className={`px-3 py-2 text-xs font-mono rounded-[2px] border transition-all text-center cursor-pointer ${
                        imageType === 'preset'
                          ? 'bg-[var(--accent-mauve)]/10 border-[var(--accent-mauve)] text-[var(--accent-mauve)] font-semibold'
                          : 'bg-[var(--bg-mantle)] border-[var(--border)] text-[var(--text-muted)] hover:border-[var(--border-focus)]'
                      }`}
                    >
                      Curated Distribution
                    </button>
                    <button
                      type="button"
                      onClick={() => setImageType('custom')}
                      className={`px-3 py-2 text-xs font-mono rounded-[2px] border transition-all text-center cursor-pointer ${
                        imageType === 'custom'
                          ? 'bg-[var(--accent-mauve)]/10 border-[var(--accent-mauve)] text-[var(--accent-mauve)] font-semibold'
                          : 'bg-[var(--bg-mantle)] border-[var(--border)] text-[var(--text-muted)] hover:border-[var(--border-focus)]'
                      }`}
                    >
                      Custom OCI Image
                    </button>
                  </div>
                </div>

                {imageType === 'preset' ? (
                  <StudioSelect
                    label={
                      <div className="flex items-center">
                        <span>Distribution Preset</span>
                        <StudioTooltip
                          section="[image]"
                          title="preset = &quot;distro:tag&quot;"
                          description="podbox automatically handles package manager configuration, baked-in guest interceptors, and user IDs for verified distributions."
                        />
                      </div>
                    }
                    value={selectedPresetDistro}
                    onChange={(val) => setSelectedPresetDistro(val)}
                    options={[
                      { value: 'fedora:44', label: 'Fedora 44 Rawhide', sublabel: 'Recommended' },
                      { value: 'fedora:43', label: 'Fedora 43' },
                      { value: 'fedora:42', label: 'Fedora 42' },
                      { value: 'archlinux:latest', label: 'Arch Linux', sublabel: 'Rolling' },
                      { value: 'ubuntu:24.04', label: 'Ubuntu 24.04 LTS (Noble)' },
                      { value: 'debian:bookworm', label: 'Debian 12 (Bookworm)' },
                      { value: 'alpine:3.20', label: 'Alpine 3.20', sublabel: 'musl/minimal' },
                    ]}
                  />
                ) : (
                  <StudioInput
                    label={
                      <div className="flex items-center">
                        <span>Custom OCI Image URI</span>
                        <StudioTooltip
                          section="[image]"
                          title="base = &quot;registry/org/image:tag&quot;"
                          description="Full registry reference for your container base image."
                        />
                      </div>
                    }
                    value={customImageBase}
                    onChange={(e) => setCustomImageBase(e.target.value)}
                    placeholder="ghcr.io/org/custom-image:latest"
                  />
                )}

                {/* Packages to install with Chip Tag Input */}
                <StudioTagInput
                  label={
                    <div className="flex items-center">
                      <span>Packages to Install</span>
                      <StudioTooltip
                        section="[image]"
                        title="packages = [&quot;pkg1&quot;, &quot;pkg2&quot;]"
                        description="Packages to bake directly into the OCI image at build time using the distribution's native package manager."
                      />
                    </div>
                  }
                  tags={packagesInstallList}
                  onChange={setPackagesInstallList}
                  placeholder="Type package and press Enter..."
                  helperText="Baked during podbox build into the immutable rootfs layer."
                />

                {/* Packages to remove */}
                <StudioTagInput
                  label={
                    <div className="flex items-center">
                      <span>Packages to Remove (Optional)</span>
                      <StudioTooltip
                        section="[image]"
                        title="remove_packages = [...]"
                        description="Unwanted stock packages purged during image synthesis to reduce size."
                      />
                    </div>
                  }
                  tags={packagesRemoveList}
                  onChange={setPackagesRemoveList}
                  placeholder="e.g. vim-minimal, nano..."
                />

                {/* Advanced Image tuning */}
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <StudioSelect
                    label={
                      <div className="flex items-center">
                        <span>Package Manager</span>
                        <StudioTooltip
                          section="[image]"
                          title="package_manager = &quot;auto&quot;"
                          description="Force package manager binary (dnf, apt, pacman, apk) or let podbox detect automatically from base OS."
                        />
                      </div>
                    }
                    value={packageManager}
                    onChange={setPackageManager}
                    options={[
                      { value: 'auto', label: 'Auto Detect (Recommended)' },
                      { value: 'dnf', label: 'DNF (Fedora / RHEL)' },
                      { value: 'apt', label: 'APT (Ubuntu / Debian)' },
                      { value: 'pacman', label: 'Pacman (Arch Linux)' },
                      { value: 'apk', label: 'APK (Alpine)' },
                    ]}
                  />

                  <StudioInput
                    label={
                      <div className="flex items-center">
                        <span>Pull Retries</span>
                        <StudioTooltip
                          section="[image]"
                          title="pull_retry = 3"
                          description="Number of times podman will retry pulling layers over flaky networks."
                        />
                      </div>
                    }
                    type="number"
                    value={pullRetry}
                    onChange={(e) => setPullRetry(parseInt(e.target.value) || 0)}
                    min={0}
                    max={10}
                  />
                </div>

                {/* Extra Run commands */}
                <div className="space-y-1.5">
                  <div className="flex items-center">
                    <span className="text-xs font-medium text-[var(--text-subtext)]">Extra Containerfile RUN Commands</span>
                    <StudioTooltip
                      section="[image]"
                      title="run = [&quot;cmd1&quot;, &quot;cmd2&quot;]"
                      description="Custom shell commands executed inside the build container to configure dotfiles, compilers, or custom software."
                      quadlet="RUN <command>"
                    />
                  </div>
                  <textarea
                    value={runCommands}
                    onChange={(e) => setRunCommands(e.target.value)}
                    rows={2}
                    placeholder="e.g. dnf clean all"
                    className="w-full px-3 py-2 text-xs font-mono rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent-mauve)] focus:ring-1 focus:ring-[var(--accent-mauve)]/30 transition-all resize-y"
                  />
                </div>
              </div>
            )}

            {/* -------------------------------------------------------------- */}
            {/* [CONTAINER] SECTION                                            */}
            {/* -------------------------------------------------------------- */}
            {activeCategory === 'container' && (
              <div className="space-y-5 animate-fadeIn">
                <div className="pb-3 border-b border-[var(--border)]">
                  <div className="flex items-center gap-2">
                    <Boxes className="w-4 h-4 text-[var(--accent-mauve)]" />
                    <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
                      [container] — Identity, Resources &amp; Mounts
                    </h2>
                  </div>
                  <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
                    Configure the container name, isolated home directory, interactive shell, CPU/memory quotas, and volume mounts.
                  </p>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <StudioInput
                    label={
                      <div className="flex items-center">
                        <span>Container Name</span>
                        <StudioTooltip
                          section="[container]"
                          title="name = &quot;dev-box&quot;"
                          description="The unique name identifying this environment. Used in systemd unit naming: ~/.config/containers/systemd/<name>.container."
                          quadlet="ContainerName=podbox-<name>"
                        />
                      </div>
                    }
                    value={containerName}
                    onChange={(e) => handleContainerNameChange(e.target.value)}
                    placeholder="dev-box"
                  />

                  <StudioInput
                    label={
                      <div className="flex items-center">
                        <span>Isolated Home Directory</span>
                        <StudioTooltip
                          section="[container]"
                          title="home = &quot;~/containers/name&quot;"
                          description="Host path mounted as the container's isolated $HOME. Keeps your host ~ clean from dotfile clutter."
                          quadlet="Volume=%h/containers/<name>:/home/user:rslave,z"
                        />
                      </div>
                    }
                    value={containerHome}
                    onChange={(e) => setContainerHome(e.target.value)}
                    placeholder="~/containers/dev-box"
                  />
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
                  <StudioSelect
                    label={
                      <div className="flex items-center">
                        <span>Default Shell</span>
                        <StudioTooltip
                          section="[container]"
                          title="shell = &quot;fish&quot;"
                          description="Default interactive shell spawned upon 'podbox enter <name>'."
                        />
                      </div>
                    }
                    value={containerShell}
                    onChange={setContainerShell}
                    options={[
                      { value: 'bash', label: '/bin/bash' },
                      { value: 'fish', label: '/usr/bin/fish' },
                      { value: 'zsh', label: '/bin/zsh' },
                      { value: 'sh', label: '/bin/sh' },
                    ]}
                  />

                  <StudioInput
                    label={
                      <div className="flex items-center">
                        <span>Memory Limit</span>
                        <StudioTooltip
                          section="[container]"
                          title="memory = &quot;4G&quot;"
                          description="Cgroup memory ceiling (e.g., 2G, 8G, 512M). Systemd enforces hard isolation to protect host stability."
                          quadlet="Memory=4G"
                        />
                      </div>
                    }
                    value={containerMemory}
                    onChange={(e) => setContainerMemory(e.target.value)}
                    placeholder="4G"
                  />

                  <StudioInput
                    label={
                      <div className="flex items-center">
                        <span>CPU Quota</span>
                        <StudioTooltip
                          section="[container]"
                          title="cpus = &quot;2.0&quot;"
                          description="Fractional CPU cores allocated to this container (2.0 = 200% cgroup CPU quota)."
                          quadlet="CpuQuota=200%"
                        />
                      </div>
                    }
                    value={containerCpus}
                    onChange={(e) => setContainerCpus(e.target.value)}
                    placeholder="2.0"
                  />
                </div>

                {/* Extra Mounts */}
                <div className="space-y-2 pt-2 border-t border-[var(--border)]">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center">
                      <span className="text-xs font-medium text-[var(--text-subtext)]">Extra Host Mounts</span>
                      <StudioTooltip
                        section="[container]"
                        title="mounts = [&quot;host:guest:mode&quot;]"
                        description="Bind mounts sharing directory trees between host and guest. Use :z or :Z for SELinux relabeling."
                        quadlet="Volume=%h/Projects:/home/user/Projects:z"
                      />
                    </div>
                    <button
                      type="button"
                      onClick={() =>
                        setExtraMounts([
                          ...extraMounts,
                          { host: '~/Downloads', guest: '/home/user/Downloads', mode: 'z' },
                        ])
                      }
                      className="text-xs text-[var(--accent-mauve)] hover:text-white flex items-center gap-1 cursor-pointer"
                    >
                      <Plus className="w-3 h-3" />
                      <span>Add Mount</span>
                    </button>
                  </div>

                  <div className="space-y-2">
                    {extraMounts.map((m, idx) => (
                      <div key={idx} className="flex items-center gap-2">
                        <input
                          type="text"
                          value={m.host}
                          onChange={(e) => {
                            const updated = [...extraMounts];
                            updated[idx].host = e.target.value;
                            setExtraMounts(updated);
                          }}
                          placeholder="Host path (e.g. ~/Projects)"
                          className="flex-1 px-2.5 py-1.5 text-xs font-mono rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent-mauve)]"
                        />
                        <span className="text-[var(--text-muted)] text-xs font-mono">→</span>
                        <input
                          type="text"
                          value={m.guest}
                          onChange={(e) => {
                            const updated = [...extraMounts];
                            updated[idx].guest = e.target.value;
                            setExtraMounts(updated);
                          }}
                          placeholder="Guest path"
                          className="flex-1 px-2.5 py-1.5 text-xs font-mono rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent-mauve)]"
                        />
                        <button
                          type="button"
                          onClick={() => setExtraMounts(extraMounts.filter((_, i) => i !== idx))}
                          className="p-1.5 text-[var(--text-muted)] hover:text-[var(--accent-red)] rounded-[2px] cursor-pointer"
                          title="Remove mount"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            )}

            {/* -------------------------------------------------------------- */}
            {/* [SECURITY] SECTION                                             */}
            {/* -------------------------------------------------------------- */}
            {activeCategory === 'security' && (
              <div className="space-y-5 animate-fadeIn">
                <div className="pb-3 border-b border-[var(--border)]">
                  <div className="flex items-center gap-2">
                    <ShieldCheck className="w-4 h-4 text-[var(--accent-mauve)]" />
                    <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
                      [security] — User Namespaces &amp; Isolation
                    </h2>
                  </div>
                  <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
                    Configure UserNS mapping, Linux capabilities, SELinux security labels, and rootfs mutability.
                  </p>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  <StudioSwitch
                    id="sec-label"
                    checked={secLabelDisable}
                    onChange={setSecLabelDisable}
                    label={
                      <div className="flex items-center">
                        <span>Disable SELinux Labeling</span>
                        <StudioTooltip
                          section="[security]"
                          title="security_label_disable = true"
                          description="Permits container processes to access files labeled with unconfined_u, essential for GPU DRI and Wayland sockets."
                          quadlet="SecurityLabelDisable=true"
                          security="Disable only when hardware or socket passthrough requires it."
                        />
                      </div>
                    }
                    description="security_label_disable = true (needed for Wayland/GPU)"
                  />

                  <StudioSwitch
                    id="sec-no-new-priv"
                    checked={noNewPrivileges}
                    onChange={setNoNewPrivileges}
                    label={
                      <div className="flex items-center">
                        <span>No New Privileges</span>
                        <StudioTooltip
                          section="[security]"
                          title="no_new_privileges = true"
                          description="Prevents processes inside the container from gaining additional privileges via setuid or setgid binaries."
                          quadlet="NoNewPrivileges=true"
                          security="Strongly recommended for all desktop and web environments."
                        />
                      </div>
                    }
                    description="Disallow setuid/setgid privilege escalation"
                  />

                  <StudioSwitch
                    id="sec-readonly"
                    checked={readOnlyRootfs}
                    onChange={setReadOnlyRootfs}
                    label={
                      <div className="flex items-center">
                        <span>Read-Only Root Filesystem</span>
                        <StudioTooltip
                          section="[security]"
                          title="read_only_rootfs = true"
                          description="Mounts / read-only inside the container. State can only be written to $HOME or explicitly mounted volumes."
                          quadlet="ReadOnly=true"
                        />
                      </div>
                    }
                    description="Prevents arbitrary writes to container OS directories"
                  />

                  <div className="space-y-1">
                    <StudioSelect
                      label={
                        <div className="flex items-center">
                          <span>UserNS Mode</span>
                          <StudioTooltip
                            section="[security]"
                            title="userns = &quot;keep-id&quot;"
                            description="Maps your host UID (1000) directly to container UID (1000) so files created on disk have your host ownership."
                            quadlet="UserNS=keep-id"
                          />
                        </div>
                      }
                      value={usernsMode}
                      onChange={setUsernsMode}
                      options={[
                        { value: 'keep-id', label: 'keep-id (Host UID = Container UID)' },
                        { value: 'nomap', label: 'nomap (Rootless subordinate IDs)' },
                        { value: 'private', label: 'private (Standard user namespace)' },
                      ]}
                    />
                  </div>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-2 border-t border-[var(--border)]">
                  <StudioSelect
                    label={
                      <div className="flex items-center">
                        <span>Capability Preset</span>
                        <StudioTooltip
                          section="[security]"
                          title="cap_preset = &quot;default&quot;"
                          description="Preconfigured bundle of Linux capabilities tailored for standard development, strict isolation, or admin tasks."
                        />
                      </div>
                    }
                    value={capPreset}
                    onChange={setCapPreset}
                    options={[
                      { value: 'default', label: 'Default (Standard rootless dev)' },
                      { value: 'none', label: 'None (Drop all capabilities)' },
                      { value: 'monitoring', label: 'Monitoring (Add SYS_PTRACE)' },
                      { value: 'admin', label: 'Admin (CAP_NET_ADMIN / SYS_ADMIN)' },
                    ]}
                  />

                  <StudioTagInput
                    label={
                      <div className="flex items-center">
                        <span>Extra Linux Capabilities (cap_add)</span>
                        <StudioTooltip
                          section="[security]"
                          title="cap_add = [&quot;SYS_PTRACE&quot;]"
                          description="Specific Linux capabilities to grant to the container processes (e.g. for gdb, perf, or bpftrace)."
                          quadlet="AddCapability=SYS_PTRACE"
                        />
                      </div>
                    }
                    tags={extraCapAddList}
                    onChange={setExtraCapAddList}
                    placeholder="e.g. SYS_PTRACE, NET_BIND_SERVICE"
                  />
                </div>
              </div>
            )}

            {/* -------------------------------------------------------------- */}
            {/* [NETWORK] SECTION                                              */}
            {/* -------------------------------------------------------------- */}
            {activeCategory === 'network' && (
              <div className="space-y-5 animate-fadeIn">
                <div className="pb-3 border-b border-[var(--border)]">
                  <div className="flex items-center gap-2">
                    <Network className="w-4 h-4 text-[var(--accent-mauve)]" />
                    <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
                      [network] — Pasta &amp; Port Forwarding
                    </h2>
                  </div>
                  <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
                    Configure network namespace isolation mode, pasta userspace stack, and published TCP/UDP ports.
                  </p>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <StudioSelect
                    label={
                      <div className="flex items-center">
                        <span>Network Mode</span>
                        <StudioTooltip
                          section="[network]"
                          title="mode = &quot;private&quot; | &quot;pasta&quot; | &quot;host&quot;"
                          description="private creates an isolated loopback; pasta provides high-performance userspace NAT; host shares host stack directly."
                          quadlet="Network=pasta"
                        />
                      </div>
                    }
                    value={netMode}
                    onChange={setNetMode}
                    options={[
                      { value: 'private', label: 'private (Isolated loopback namespace)' },
                      { value: 'pasta', label: 'pasta (Podman userspace networking)' },
                      { value: 'host', label: 'host (Direct host network stack)' },
                      { value: 'bridge', label: 'bridge (Podman default CNI bridge)' },
                      { value: 'none', label: 'none (Completely offline / airgapped)' },
                    ]}
                  />

                  <StudioTagInput
                    label={
                      <div className="flex items-center">
                        <span>Publish Ports (host:container)</span>
                        <StudioTooltip
                          section="[network]"
                          title="ports = [&quot;8080:80&quot;, &quot;3000:3000&quot;]"
                          description="Forwards incoming host network ports to listening services inside the container."
                          quadlet="PublishPort=8080:80"
                        />
                      </div>
                    }
                    tags={portMappingsList}
                    onChange={setPortMappingsList}
                    placeholder="e.g. 3000:3000, 8080:80"
                    helperText="Ignored when network mode is set to host."
                  />
                </div>
              </div>
            )}

            {/* -------------------------------------------------------------- */}
            {/* [INTEGRATION] SECTION                                          */}
            {/* -------------------------------------------------------------- */}
            {activeCategory === 'integration' && (
              <div className="space-y-5 animate-fadeIn">
                <div className="pb-3 border-b border-[var(--border)]">
                  <div className="flex items-center gap-2">
                    <Monitor className="w-4 h-4 text-[var(--accent-mauve)]" />
                    <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
                      [integration] — Wayland, Audio, GPU &amp; Desktop
                    </h2>
                  </div>
                  <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
                    Seamless Linux desktop passthrough: Wayland display socket, PipeWire audio, DRI GPU acceleration, and desktop sharing.
                  </p>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  <StudioSwitch
                    id="int-wayland"
                    checked={intWayland}
                    onChange={setIntWayland}
                    label={
                      <div className="flex items-center">
                        <span>Wayland Display Socket</span>
                        <StudioTooltip
                          section="[integration]"
                          title="wayland = true"
                          description="Passes through WAYLAND_DISPLAY and mounts /run/user/1000/wayland-0 so GUI apps render natively on host compositor."
                          quadlet="Volume=/run/user/%U/wayland-0:/run/user/1000/wayland-0:ro"
                        />
                      </div>
                    }
                    description="Run graphical Wayland applications on host compositor"
                  />

                  <StudioSwitch
                    id="int-audio"
                    checked={intAudio}
                    onChange={setIntAudio}
                    label={
                      <div className="flex items-center">
                        <span>PipeWire &amp; PulseAudio</span>
                        <StudioTooltip
                          section="[integration]"
                          title="audio = true"
                          description="Mounts /run/user/1000/pipewire-0 and pulse sockets for low-latency host audio playback and capture."
                          quadlet="Volume=/run/user/%U/pipewire-0:/run/user/1000/pipewire-0:ro"
                        />
                      </div>
                    }
                    description="Full host audio playback and microphone capture"
                  />

                  <StudioSwitch
                    id="int-dbus"
                    checked={intDbus}
                    onChange={setIntDbus}
                    label={
                      <div className="flex items-center">
                        <span>Filtered D-Bus Proxy</span>
                        <StudioTooltip
                          section="[integration]"
                          title="dbus = true"
                          description="Spawns an xdg-dbus-proxy filtering access to notifications, portals, and media controls without exposing system bus."
                        />
                      </div>
                    }
                    description="xdg-dbus-proxy filtered session bus communication"
                  />

                  <StudioSwitch
                    id="int-notify"
                    checked={intNotify}
                    onChange={setIntNotify}
                    label={
                      <div className="flex items-center">
                        <span>Desktop Notifications</span>
                        <StudioTooltip
                          section="[integration]"
                          title="notify = true"
                          description="Allows container utilities (e.g. notify-send) to pop desktop notifications onto host screen."
                        />
                      </div>
                    }
                    description="Forward notify-send alerts to host notification server"
                  />

                  <StudioSwitch
                    id="int-clipboard"
                    checked={intClipboard}
                    onChange={setIntClipboard}
                    label={
                      <div className="flex items-center">
                        <span>Clipboard Sharing</span>
                        <StudioTooltip
                          section="[integration]"
                          title="clipboard = true"
                          description="Permits copy/paste synchronization between container terminal/apps and host desktop."
                        />
                      </div>
                    }
                    description="Seamless copy & paste between host and container"
                  />

                  <StudioSwitch
                    id="int-ssh-agent"
                    checked={intSshAgent}
                    onChange={setIntSshAgent}
                    label={
                      <div className="flex items-center">
                        <span>Forward SSH Agent</span>
                        <StudioTooltip
                          section="[integration]"
                          title="ssh_agent = true"
                          description="Mounts SSH_AUTH_SOCK into container so git operations can use host SSH keys without copying private keys."
                        />
                      </div>
                    }
                    description="Use host SSH keys for git clone/push securely"
                  />
                </div>

                <div className="pt-2 border-t border-[var(--border)]">
                  <StudioSelect
                    label={
                      <div className="flex items-center">
                        <span>Hardware GPU Acceleration</span>
                        <StudioTooltip
                          section="[integration]"
                          title="gpu = &quot;auto&quot; | &quot;nvidia&quot; | true | false"
                          description="Direct rendering infrastructure (/dev/dri) passthrough for Vulkan, OpenGL, and compute workloads."
                          quadlet="Device=/dev/dri"
                        />
                      </div>
                    }
                    value={intGpu}
                    onChange={setIntGpu}
                    options={[
                      { value: 'auto', label: 'auto (Pass /dev/dri if present on host)' },
                      { value: 'nvidia', label: 'nvidia (NVIDIA Container Toolkit CDI)' },
                      { value: 'true', label: 'true (Require /dev/dri passthrough)' },
                      { value: 'false', label: 'false (Software rendering only)' },
                    ]}
                  />
                </div>
              </div>
            )}

            {/* -------------------------------------------------------------- */}
            {/* [LIFECYCLE] SECTION                                            */}
            {/* -------------------------------------------------------------- */}
            {activeCategory === 'lifecycle' && (
              <div className="space-y-5 animate-fadeIn">
                <div className="pb-3 border-b border-[var(--border)]">
                  <div className="flex items-center gap-2">
                    <RefreshCw className="w-4 h-4 text-[var(--accent-mauve)]" />
                    <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
                      [lifecycle] &amp; [systemd] — Quadlet Unit &amp; Autostart
                    </h2>
                  </div>
                  <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
                    Configure Quadlet unit generation, systemd user service startup, auto-updates, and restart behavior.
                  </p>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  <StudioSwitch
                    id="life-quadlet"
                    checked={lifeQuadlet}
                    onChange={setLifeQuadlet}
                    label={
                      <div className="flex items-center">
                        <span>Synthesize Quadlet Unit</span>
                        <StudioTooltip
                          section="[lifecycle]"
                          title="quadlet = true"
                          description="Generates ~/.config/containers/systemd/<name>.container file so systemd natively orchestrates this container."
                        />
                      </div>
                    }
                    description="quadlet = true (write unit file on podbox build)"
                  />

                  <StudioSwitch
                    id="life-autostart"
                    checked={lifeAutostart}
                    onChange={setLifeAutostart}
                    label={
                      <div className="flex items-center">
                        <span>Autostart on Login</span>
                        <StudioTooltip
                          section="[lifecycle]"
                          title="autostart = true"
                          description="Configures WantedBy=default.target in the Quadlet unit so container starts immediately upon user login."
                          quadlet="[Install]\nWantedBy=default.target"
                        />
                      </div>
                    }
                    description="Start container automatically when user logs in"
                  />

                  <StudioSwitch
                    id="life-autoupdate"
                    checked={lifeAutoUpdate}
                    onChange={setLifeAutoUpdate}
                    label={
                      <div className="flex items-center">
                        <span>Registry Auto-Update</span>
                        <StudioTooltip
                          section="[lifecycle]"
                          title="auto_update = true"
                          description="Attaches io.containers.autoupdate=registry label, allowing 'podman auto-update' timer to pull latest image."
                          quadlet="Label=io.containers.autoupdate=registry"
                        />
                      </div>
                    }
                    description="Check upstream registry and restart on new image"
                  />

                  <div className="space-y-1">
                    <StudioSelect
                      label={
                        <div className="flex items-center">
                          <span>On Stop Behavior</span>
                          <StudioTooltip
                            section="[lifecycle]"
                            title="on_stop = &quot;keep&quot; | &quot;remove&quot;"
                            description="Determines whether container layers are kept intact or removed on systemctl stop."
                          />
                        </div>
                      }
                      value={lifeOnStop}
                      onChange={setLifeOnStop}
                      options={[
                        { value: 'keep', label: 'keep (Preserve container on stop)' },
                        { value: 'remove', label: 'remove (Destroy container on stop)' },
                      ]}
                    />
                  </div>
                </div>

                <div className="pt-2 border-t border-[var(--border)]">
                  <StudioInput
                    label={
                      <div className="flex items-center">
                        <span>Systemd Unit Dependency (After=)</span>
                        <StudioTooltip
                          section="[systemd]"
                          title="after = [&quot;network-online.target&quot;]"
                          description="Ensures container starts strictly after required systemd targets are reached."
                          quadlet="After=network-online.target"
                        />
                      </div>
                    }
                    value={sysAfter}
                    onChange={(e) => setSysAfter(e.target.value)}
                    placeholder="network-online.target"
                  />
                </div>
              </div>
            )}

            {/* -------------------------------------------------------------- */}
            {/* [DBUS] SECTION                                                 */}
            {/* -------------------------------------------------------------- */}
            {activeCategory === 'dbus' && (
              <div className="space-y-5 animate-fadeIn">
                <div className="pb-3 border-b border-[var(--border)]">
                  <div className="flex items-center gap-2">
                    <Radio className="w-4 h-4 text-[var(--accent-mauve)]" />
                    <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
                      [dbus] — xdg-dbus-proxy Filter Rules
                    </h2>
                  </div>
                  <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
                    Configure granular D-Bus session bus access rules for desktop portals, notifications, and media keys.
                  </p>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <StudioSelect
                    label={
                      <div className="flex items-center">
                        <span>D-Bus Security Preset</span>
                        <StudioTooltip
                          section="[dbus]"
                          title="preset = &quot;portal&quot;"
                          description="Preconfigured bus filtering rules matching Flatpak sandbox expectations."
                        />
                      </div>
                    }
                    value={dbusPreset}
                    onChange={setDbusPreset}
                    options={[
                      { value: 'portal', label: 'portal (Desktop Portals & Notifications)' },
                      { value: 'gnome', label: 'gnome (GNOME Shell integration)' },
                      { value: 'kde', label: 'kde (KDE Plasma integration)' },
                      { value: 'flatpak', label: 'flatpak (Strict Flatpak compatibility)' },
                      { value: 'none', label: 'none (Custom rules only)' },
                    ]}
                  />

                  <StudioTagInput
                    label={
                      <div className="flex items-center">
                        <span>Talk Bus Names (Allowed Messages)</span>
                        <StudioTooltip
                          section="[dbus]"
                          title="talk = [&quot;org.freedesktop.Notifications&quot;]"
                          description="D-Bus bus names the container is permitted to call methods on."
                        />
                      </div>
                    }
                    tags={dbusTalkList}
                    onChange={setDbusTalkList}
                    placeholder="e.g. org.freedesktop.Notifications"
                  />
                </div>
              </div>
            )}

            {/* -------------------------------------------------------------- */}
            {/* [WAYLAND] SECTION                                              */}
            {/* -------------------------------------------------------------- */}
            {activeCategory === 'wayland' && (
              <div className="space-y-5 animate-fadeIn">
                <div className="pb-3 border-b border-[var(--border)]">
                  <div className="flex items-center gap-2">
                    <Layers3 className="w-4 h-4 text-[var(--accent-mauve)]" />
                    <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
                      [wayland] — Protocol Firewall
                    </h2>
                  </div>
                  <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
                    Intercept and filter dangerous Wayland globals (e.g., screencast/keylogger protocols) for enhanced GUI security.
                  </p>
                </div>

                <StudioSwitch
                  id="wayland-firewall"
                  checked={waylandFirewall}
                  onChange={setWaylandFirewall}
                  label={
                    <div className="flex items-center">
                      <span>Enable Wayland Protocol Firewall</span>
                      <StudioTooltip
                        section="[wayland]"
                        title="firewall = true"
                        description="Filters wl_registry globals advertised by host compositor to block untrusted apps from taking screenshots or capturing keystrokes."
                      />
                    </div>
                  }
                  description="Block sensitive compositor interfaces from untrusted container apps"
                />

                <StudioTagInput
                  label={
                    <div className="flex items-center">
                      <span>Blocked Wayland Interfaces</span>
                      <StudioTooltip
                        section="[wayland]"
                        title="blocked_interfaces = [...]"
                        description="Specific Wayland global interface strings hidden from container applications."
                      />
                    </div>
                  }
                  tags={waylandBlockedList}
                  onChange={setWaylandBlockedList}
                  placeholder="e.g. zwlr_screencopy_manager_v1"
                />
              </div>
            )}
          </div>
        </div>

        {/* ------------------------------------------------------------------ */}
        {/* RIGHT COLUMN: Real-Time Synthesizer Output Pane (5 cols)           */}
        {/* ------------------------------------------------------------------ */}
        <div className="lg:col-span-5 flex flex-col min-h-0 h-full overflow-hidden rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)] shadow-sm">
          {/* Header tabs for Output */}
          <div className="shrink-0 flex items-center justify-between bg-[var(--bg-crust)] px-3 py-2 border-b border-[var(--border)]">
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                onClick={() => setActiveView('toml')}
                className={`px-3 py-1.5 rounded-[2px] text-xs font-mono font-medium transition-colors cursor-pointer flex items-center gap-1.5 ${
                  activeView === 'toml'
                    ? 'bg-[var(--accent-mauve)] text-[var(--bg-crust)] font-bold shadow-xs'
                    : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]'
                }`}
              >
                <FileCode2 className="w-3.5 h-3.5" />
                <span>podbox.toml</span>
              </button>

              <button
                type="button"
                onClick={() => setActiveView('quadlet')}
                className={`px-3 py-1.5 rounded-[2px] text-xs font-mono font-medium transition-colors cursor-pointer flex items-center gap-1.5 ${
                  activeView === 'quadlet'
                    ? 'bg-[var(--accent-peach)] text-[var(--bg-crust)] font-bold shadow-xs'
                    : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]'
                }`}
              >
                <Terminal className="w-3.5 h-3.5" />
                <span>{containerName || 'podbox'}.container</span>
              </button>
            </div>

            <div className="flex items-center gap-1.5">
              <button
                onClick={activeView === 'toml' ? handleDownloadToml : handleDownloadQuadlet}
                type="button"
                className="px-2 py-1 text-xs rounded-[2px] bg-[var(--bg-surface0)] hover:bg-[var(--bg-surface1)] text-[var(--text-primary)] transition-colors cursor-pointer flex items-center gap-1 font-mono"
                title={`Download ${activeView === 'toml' ? `${containerName || 'podbox'}.toml` : `${containerName || 'podbox'}.container`}`}
              >
                <Download className="w-3.5 h-3.5 text-[var(--accent-blue)]" />
                <span className="text-[11px] hidden sm:inline">Save</span>
              </button>

              <button
                onClick={handleCopyConfig}
                type="button"
                className="px-2.5 py-1 text-xs rounded-[2px] bg-[var(--bg-surface0)] hover:bg-[var(--bg-surface1)] text-[var(--text-primary)] transition-colors cursor-pointer flex items-center gap-1 font-mono"
                title="Copy to clipboard"
              >
                {copied ? (
                  <>
                    <Check className="w-3.5 h-3.5 text-[var(--accent-green)]" />
                    <span className="text-[var(--accent-green)] text-[11px]">Copied</span>
                  </>
                ) : (
                  <>
                    <Copy className="w-3.5 h-3.5 text-[var(--accent-mauve)]" />
                    <span className="text-[11px]">Copy</span>
                  </>
                )}
              </button>
            </div>
          </div>

          {/* Subheader info bar */}
          <div className="shrink-0 px-3 py-1.5 bg-[var(--bg-surface0)]/40 border-b border-[var(--border)] flex items-center justify-between text-[11px] font-mono text-[var(--text-muted)]">
            <span className="truncate">
              {activeView === 'toml'
                ? 'Declarative Podbox Spec'
                : `systemd Unit (~/.config/containers/systemd/${containerName || 'podbox'}.container)`}
            </span>
            <span className="text-[10px] uppercase font-bold text-[var(--accent-mauve)] shrink-0 ml-2">
              Live Synthesizer
            </span>
          </div>

          {/* Code Body - flex-1 min-h-0 overflow-auto */}
          <div className="flex-1 min-h-0 overflow-auto p-3.5 font-mono text-[12.5px] leading-relaxed select-text custom-scrollbar bg-[var(--bg-crust)]/50">
            <div className="table w-full">
              {codeLines.map((line, idx) => (
                <div key={idx} className="table-row hover:bg-[var(--bg-surface0)]/40 transition-colors">
                  <span className="table-cell pr-4 pl-1 text-right select-none text-[var(--text-muted)]/40 text-[11px] w-9">
                    {idx + 1}
                  </span>
                  <span className="table-cell whitespace-pre font-mono">
                    {highlightCodeLine(line, activeView)}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {/* Bottom Bar: Line count & CLI Hint */}
          <div className="shrink-0 px-3 py-2 bg-[var(--bg-crust)] border-t border-[var(--border)] flex flex-col sm:flex-row sm:items-center justify-between gap-1.5 text-[11px] font-mono">
            <div className="flex items-center gap-2 text-[var(--text-muted)]">
              <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent-green)]" />
              <span>{codeLines.length} lines</span>
              <span className="text-[var(--border)]">|</span>
              <span className="text-[var(--text-subtext)]">{activeView === 'toml' ? 'podbox.toml' : 'systemd unit'}</span>
            </div>
            <div className="text-[10px] text-[var(--accent-teal)] truncate">
              $ podbox build . && podbox enter {containerName || 'dev-box'}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
