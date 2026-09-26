export interface PlaygroundConfig {
  name: string;
  distro: 'fedora' | 'arch' | 'ubuntu' | 'debian' | 'custom';
  customImage: string;
  wayland: boolean;
  pipewire: boolean;
  gpu: 'auto' | 'nvidia' | 'off';
  dbusNotifications: boolean;
  shareProjects: boolean;
  packages: string;
}

const DISTRO_BASES: Record<string, string> = {
  fedora: 'fedora:44',
  arch: 'archlinux:latest',
  ubuntu: 'ubuntu:24.04',
  debian: 'debian:bookworm',
};

export function generatePlaygroundToml(cfg: PlaygroundConfig): string {
  const { name, distro, customImage, wayland, pipewire, gpu, dbusNotifications, shareProjects } = cfg;
  const parsedPackages = cfg.packages
    .split(',')
    .map((p) => p.trim())
    .filter(Boolean);

  let toml = `# podbox.toml for ${name}\n`;
  toml += `[container]\n`;
  toml += `name = "${name}"\n`;
  toml += `home = "~/containers/${name}"\n\n`;

  toml += `[image]\n`;
  toml += `base = "${distro === 'custom' ? customImage : DISTRO_BASES[distro]}"\n`;
  if (parsedPackages.length > 0) {
    toml += `packages = [${parsedPackages.map((p) => `"${p}"`).join(', ')}]\n`;
  }

  toml += `\n[integration]\n`;
  toml += `wayland = ${wayland}\n`;
  toml += `audio = ${pipewire}\n`;
  toml += `gpu = ${gpu === 'off' ? 'false' : `"${gpu}"`}\n`;
  toml += `dbus = ${dbusNotifications}\n`;

  if (shareProjects) {
    toml += `\n[integration.xdg_dirs]\n`;
    toml += `projects = true\n`;
  }

  toml += `\n[lifecycle]\n`;
  toml += `quadlet = true\n`;
  toml += `on_stop = "keep"\n`;

  return toml;
}

export function generatePlaygroundQuadlet(cfg: PlaygroundConfig): string {
  const { name, wayland, pipewire, gpu, dbusNotifications, shareProjects } = cfg;

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
