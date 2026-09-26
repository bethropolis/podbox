import React from 'react';
import {
  Boxes,
  Plus,
  Trash2,
} from 'lucide-react';
import {
  StudioInput,
  StudioSelect,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import type { StudioState } from '../useStudioState';

type ContainerPanelProps = Pick<StudioState, 'containerCpus' | 'containerHome' | 'containerMemory' | 'containerName' | 'containerShell' | 'extraMounts' | 'handleContainerNameChange' | 'setContainerCpus' | 'setContainerHome' | 'setContainerMemory' | 'setContainerShell' | 'setExtraMounts'>;

export function ContainerPanel({ st }: { st: ContainerPanelProps }) {
  const { containerCpus, containerHome, containerMemory, containerName, containerShell, extraMounts, handleContainerNameChange, setContainerCpus, setContainerHome, setContainerMemory, setContainerShell, setExtraMounts } = st;
  return (
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
  );
}
