import { tomlString } from './toml';

export interface PlaygroundConfig {
  name: string;
  distro: 'fedora' | 'arch' | 'ubuntu' | 'debian' | 'custom';
  customImage: string;
  packages: string[];
  packagesRemove: string[];
  run: string[];
  env: Record<string, string>;
  mounts: Array<{ host: string; container: string }>;
  ports: Array<{ host: string; container: string }>;
  gpu: 'off' | 'nvidia' | 'intel' | 'amd';
  memory: string;
  cpus: string;
  network: 'bridge' | 'host' | 'none' | 'slirp4netns';
}

const DISTRO_BASES: Record<string, string> = {
  fedora: 'quay.io/fedora/fedora:latest',
  arch: 'archlinux:latest',
  ubuntu: 'ubuntu:24.04',
  debian: 'debian:bookworm-slim',
};

export function generatePlaygroundToml(cfg: PlaygroundConfig): string {
  let toml = '';
  toml += `[container]\n`;
  toml += `name = ${tomlString(cfg.name)}\n`;
  toml += `home = ${tomlString(`~/containers/${cfg.name}`)}\n`;
  toml += `memory = ${cfg.memory ? tomlString(cfg.memory) : '""'}\n`;
  toml += `cpus = ${cfg.cpus ? tomlString(cfg.cpus) : '""'}\n`;
  toml += `network = ${tomlString(cfg.network)}\n\n`;

  toml += `[image]\n`;
  const base = cfg.distro === 'custom' ? cfg.customImage : DISTRO_BASES[cfg.distro];
  toml += `base = ${tomlString(base)}\n`;
  if (cfg.packages.length) toml += `packages = [${cfg.packages.map((p) => tomlString(p)).join(', ')}]\n`;
  if (cfg.packagesRemove.length) toml += `packages.remove = [${cfg.packagesRemove.map((p) => tomlString(p)).join(', ')}]\n`;
  if (cfg.run.length) toml += `run = [${cfg.run.map((r) => tomlString(r)).join(', ')}]\n`;
  toml += `\n`;

  if (Object.keys(cfg.env).length) {
    toml += `[env]\n`;
    for (const [k, v] of Object.entries(cfg.env)) toml += `${tomlString(k)} = ${tomlString(v)}\n`;
    toml += `\n`;
  }

  if (cfg.mounts.length) {
    toml += `[mounts]\n`;
    for (const m of cfg.mounts) toml += `${tomlString(m.host)} = ${tomlString(m.container)}\n`;
    toml += `\n`;
  }

  if (cfg.ports.length) {
    toml += `[ports]\n`;
    for (const p of cfg.ports) toml += `${tomlString(p.host)} = ${tomlString(p.container)}\n`;
    toml += `\n`;
  }

  if (cfg.gpu !== 'off') {
    toml += `[gpu]\n`;
    toml += `gpu = ${tomlString(cfg.gpu)}\n\n`;
  }

  return toml;
}

export function generatePlaygroundQuadlet(cfg: PlaygroundConfig): string {
  const { name, wayland, pipewire, gpu, dbusNotifications, shareProjects } = cfg as any;

  let quadlet = `# ~/.config/containers/systemd/${name}.container\n`;
  quadlet += `[Unit]\n`;
  quadlet += `Description=podbox ${name} container\n`;
  quadlet += `After=network-online.target\n\n`;

  quadlet += `[Container]\n`;
  quadlet += `Image=localhost/podbox-${name}:latest\n`;
  quadlet += `ContainerName=podbox-${name}\n`;
  quadlet += `Volume=podbox-${name}-home:/home/user:Z\n`;
  quadlet += `Volume=%h/containers/${name}:/home/user:rslave,z\n`;

  if (shareProjects) {
    quadlet += `Volume=%h/Projects:/home/user/Projects:z\n`;
  }

  if (wayland) {
    quadlet += `Environment=WAYLAND_DISPLAY=wayland-0\n`;
    quadlet += `Volume=/run/user/%U/wayland-0:/run/user/1000/wayland-0:ro\n`;
  }

  if (pipewire) {
    quadlet += `Volume=/run/user/%U/pipewire-0:/run/user/1000/pipewire-0:ro\n`;
  }

  if (gpu === 'auto') {
    quadlet += `Device=/dev/dri\n`;
  } else if (gpu === 'nvidia') {
    quadlet += `AddDevice=nvidia.com/gpu=all\n`;
  }

  if (dbusNotifications) {
    quadlet += `Volume=/run/user/%U/podbox-${name}-dbus/bus:/run/user/1000/bus:ro\n`;
  }

  quadlet += `SecurityLabelDisable=true\n`;
  quadlet += `NoNewPrivileges=true\n`;
  quadlet += `UserNS=keep-id\n\n`;

  quadlet += `[Service]\n`;
  quadlet += `Restart=on-failure\n`;
  quadlet += `TimeoutStopSec=30\n\n`;

  quadlet += `[Install]\n`;
  quadlet += `WantedBy=default.target\n`;

  return quadlet;
}
