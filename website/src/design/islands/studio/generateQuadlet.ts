import type { StudioValues } from './useStudioState';
import type { MountItem, EnvVarItem, HostExecItem } from './types';

export function generateStudioQuadlet(s: StudioValues): string {
    let q = `# ~/.config/containers/systemd/${s.containerName}.container\n`;
    q += `[Unit]\n`;
    q += `Description=podbox ${s.containerName} container\n`;
    const afterList = s.sysAfter.split(',').map((s) => s.trim()).filter(Boolean);
    if (afterList.length > 0) {
      q += `After=${afterList.join(' ')}\n`;
    }
    const reqList = s.sysRequires.split(',').map((s) => s.trim()).filter(Boolean);
    if (reqList.length > 0) {
      q += `Requires=${reqList.join(' ')}\n`;
    }

    q += `\n[Container]\n`;
    q += `Image=localhost/podbox-${s.imageName || s.containerName}:latest\n`;
    q += `ContainerName=podbox-${s.containerName}\n`;
    q += `Volume=podbox-${s.containerName}-home:/home/user:Z\n`;

    if (s.containerHome.startsWith('~/')) {
      const sub = s.containerHome.replace(/^~\/?/, '');
      q += `Volume=%h/${sub}:/home/user:rslave,z\n`;
    }

    s.extraMounts.forEach((m) => {
      if (m.host && m.guest) {
        const expandedHost = m.host.replace(/^~\//, '%h/');
        q += `Volume=${expandedHost}:${m.guest}:${m.mode || 'z'}\n`;
      }
    });

    if (s.intWayland) {
      q += `Environment=WAYLAND_DISPLAY=wayland-0\n`;
      q += `Volume=/run/user/%U/wayland-0:/run/user/1000/wayland-0:ro\n`;
    }
    if (s.intAudio) {
      q += `Volume=/run/user/%U/pulse:/run/user/1000/pulse:ro\n`;
      q += `Volume=/run/user/%U/pipewire-0:/run/user/1000/pipewire-0:ro\n`;
    }
    if (s.intGpu === 'auto' || s.intGpu === 'true' || s.intGpu === 'nvidia') {
      q += `Device=/dev/dri\n`;
      if (s.intGpu === 'nvidia') {
        q += `AddDevice=nvidia.com/gpu=all\n`;
      }
    }
    if (s.intDbus) {
      q += `Volume=/run/user/%U/podbox-${s.containerName}-dbus/bus:/run/user/1000/bus:ro\n`;
    }

    if (s.netMode !== 'private') {
      q += `Network=${s.netMode}\n`;
    }
    if (s.netMode !== 'host') {
      s.portMappingsList.forEach((p) => {
        q += `PublishPort=${p}\n`;
      });
    }

    if (s.containerMemory) q += `Memory=${s.containerMemory}\n`;
    if (s.containerCpus) q += `CpuQuota=${Math.round(parseFloat(s.containerCpus) * 100)}%\n`;
    if (s.containerReloadCmd) q += `ReloadCmd=${s.containerReloadCmd}\n`;

    if (s.secLabelDisable) q += `SecurityLabelDisable=true\n`;
    if (s.noNewPrivileges) q += `NoNewPrivileges=true\n`;
    if (s.readOnlyRootfs) q += `ReadOnly=true\n`;
    if (s.usernsMode) q += `UserNS=${s.usernsMode}\n`;
    if (s.apparmor) q += `AppArmor=${s.apparmor}\n`;
    if (s.seccomp && s.seccomp !== 'default') q += `SeccompProfile=${s.seccomp}\n`;

    if (s.lifeAutoUpdate) q += `Label=io.containers.autoupdate=registry\n`;

    s.envVars.forEach((e) => {
      if (e.key) q += `Environment=${e.key}=${e.value}\n`;
    });

    q += `\n[Service]\n`;
    q += `Restart=${s.lifeOnStop === 'remove' ? 'no' : 'on-failure'}\n`;
    q += `TimeoutStopSec=30\n\n`;

    q += `[Install]\n`;
    q += `WantedBy=${s.lifeAutostart ? 'default.target' : 'multi-user.target'}\n`;

    return q;
}
