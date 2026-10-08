import React from 'react';
import {
  Boxes,
  Plus,
  Trash2,
} from 'lucide-react';
import {
  StudioInput,
  StudioSelect,
  StudioTagInput,
  STUDIO_FIELD,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import { SHELL_OPTIONS, SERVICE_RESTART_OPTIONS } from '../schema';
import type { StudioState } from '../useStudioState';

type ContainerPanelProps = Pick<StudioState, 'containerCpus' | 'containerCpuWeight' | 'containerHome' | 'containerMemory' | 'containerName' | 'containerShell' | 'containerSlice' | 'envForward' | 'envVars' | 'handleContainerNameChange' | 'services' | 'setContainerCpus' | 'setContainerCpuWeight' | 'setContainerHome' | 'setContainerMemory' | 'setContainerShell' | 'setContainerSlice' | 'setEnvForward' | 'setEnvVars' | 'setServices'>;

export function ContainerPanel({ st, errorMap }: { st: ContainerPanelProps; errorMap?: Record<string, string> }) {
  const { containerCpus, containerCpuWeight, containerHome, containerMemory, containerName, containerShell, containerSlice, envForward, envVars, handleContainerNameChange, services, setContainerCpus, setContainerCpuWeight, setContainerHome, setContainerMemory, setContainerShell, setContainerSlice, setEnvForward, setEnvVars, setServices } = st;
  const err = (field: string) => errorMap?.[field];
  return (
<div className="space-y-5 animate-fadeIn">
  <div className="pb-3 border-b border-[var(--border)]">
    <div className="flex items-center gap-2">
      <Boxes className="w-4 h-4 text-[var(--accent-mauve)]" />
      <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
        [container] — Identity, Resources &amp; Services
      </h2>
    </div>
    <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
      Name, home folder, shell, resource limits, and background services. Extra mounts live under [storage].
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
            description="Names the container, its home folder, and its systemd unit."
            quadlet="ContainerName=podbox-<name>"
          />
        </div>
      }
      value={containerName}
      onChange={(e) => handleContainerNameChange(e.target.value)}
      placeholder="dev-box"
      id="studio-input-container-name"
      error={err('container.name')}
    />

    <StudioInput
      label={
        <div className="flex items-center">
          <span>Isolated Home Directory</span>
          <StudioTooltip
            section="[container]"
            title="home = &quot;~/containers/name&quot;"
            description="Isolated home folder. Keeps your real ~ clean."
            quadlet="Volume=%h/containers/<name>:/home/user:rslave,z"
          />
        </div>
      }
      value={containerHome}
      onChange={(e) => setContainerHome(e.target.value)}
      placeholder="~/containers/dev-box"
      id="studio-input-container-home"
      error={err('container.home')}
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
            description="Shell opened by podbox enter."
          />
        </div>
      }
      value={containerShell}
      onChange={setContainerShell}
      options={SHELL_OPTIONS}
    />

    <StudioInput
      label={
        <div className="flex items-center">
          <span>Memory Limit</span>
          <StudioTooltip
            section="[container]"
            title="memory = &quot;4G&quot;"
            description="Memory cap, e.g. 4G."
            quadlet="Memory=4G"
          />
        </div>
      }
      value={containerMemory}
      onChange={(e) => setContainerMemory(e.target.value)}
      placeholder="4G"
      id="studio-input-container-memory"
      error={err('container.memory')}
    />

    <StudioInput
      label={
        <div className="flex items-center">
          <span>CPU Quota</span>
          <StudioTooltip
            section="[container]"
            title="cpus = &quot;2.0&quot;"
            description="CPU cores, e.g. 2.0. Fractions allowed."
            quadlet="CpuQuota=200%"
          />
        </div>
      }
      value={containerCpus}
      onChange={(e) => setContainerCpus(e.target.value)}
      placeholder="2.0"
      id="studio-input-container-cpus"
      error={err('container.cpus')}
    />
  </div>

  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
    <StudioInput
      label={
        <div className="flex items-center">
          <span>Systemd Slice</span>
          <StudioTooltip
            section="[container]"
            title="slice = &quot;podbox.slice&quot;"
            description="Systemd slice — which cgroup the container lives in."
          />
        </div>
      }
      value={containerSlice}
      onChange={(e) => setContainerSlice(e.target.value)}
      placeholder="podbox.slice"
      id="studio-input-container-slice"
      error={err('container.slice')}
    />

    <StudioInput
      label={
        <div className="flex items-center">
          <span>CPU Weight</span>
          <StudioTooltip
            section="[container]"
            title="cpu_weight = 200"
            description="CPU priority when contended, 1–10000."
          />
        </div>
      }
      type="number"
      inputMode="numeric"
      value={containerCpuWeight}
      onChange={(e) => setContainerCpuWeight(parseInt(e.target.value) || 0)}
      min={1}
      max={10000}
      id="studio-input-container-cpu-weight"
      error={err('container.cpu_weight')}
    />
  </div>

  {/* Environment variables */}
  <div className="space-y-3 pt-2 border-t border-[var(--border)]">
    <div className="flex items-center justify-between">
      <div className="flex items-center">
        <span className="text-xs font-medium text-[var(--text-subtext)]">Environment Variables</span>
        <StudioTooltip
          section="[container.env]"
          title='RUST_LOG = "debug"'
          description="Always set, on every start. Never written into the image."
          quadlet="Environment=RUST_LOG=debug"
        />
      </div>
      <button
        type="button"
        onClick={() => setEnvVars([...envVars, { key: '', value: '' }])}
        className="text-xs text-[var(--accent-mauve)] hover:text-white flex items-center gap-1 cursor-pointer"
      >
        <Plus className="w-3 h-3" />
        <span>Add Variable</span>
      </button>
    </div>

    <div className="space-y-2">
      {envVars.map((v, idx) => (
        <div key={idx} className="flex items-center gap-2">
          <input
            type="text"
            value={v.key}
            aria-label={`Environment variable ${idx + 1} name`}
            spellCheck={false}
            onChange={(e) => {
              const next = [...envVars];
              next[idx] = { ...next[idx], key: e.target.value };
              setEnvVars(next);
            }}
            placeholder="VARIABLE"
            className={`${STUDIO_FIELD} w-40 shrink-0`}
          />
          <span className="text-[var(--text-muted)] text-xs font-mono">=</span>
          <input
            type="text"
            value={v.value}
            aria-label={`Environment variable ${idx + 1} value`}
            spellCheck={false}
            onChange={(e) => {
              const next = [...envVars];
              next[idx] = { ...next[idx], value: e.target.value };
              setEnvVars(next);
            }}
            placeholder="value"
            className={`${STUDIO_FIELD} flex-1`}
          />
          <button
            type="button"
            onClick={() => setEnvVars(envVars.filter((_, i) => i !== idx))}
            className="p-1.5 text-[var(--text-muted)] hover:text-[var(--accent-red)] rounded-[2px] cursor-pointer shrink-0"
            title={`Remove ${v.key || 'variable'}`}
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
        </div>
      ))}
      {envVars.length === 0 && (
        <p className="text-[11px] text-[var(--text-muted)] italic">
          No variables — the container inherits only podbox's own environment.
        </p>
      )}
    </div>

    <StudioTagInput
      label={
        <div className="flex items-center">
          <span>Forward From Host</span>
          <StudioTooltip
            section="[container.env]"
            title='forward = ["SSH_AUTH_SOCK", "AWS_*"]'
            description="Host variables copied in on enter, exec, and run — read live from your shell, so tokens stay fresh. Exact names or PREFIX_* patterns."
          />
        </div>
      }
      tags={envForward}
      onChange={setEnvForward}
      placeholder="SSH_AUTH_SOCK, AWS_*, ..."
      helperText="Forwarded on every enter, exec and run. Never written to the image."
    />
  </div>

  {/* Supervised background services */}
  <div className="space-y-2 pt-2 border-t border-[var(--border)]">
    <div className="flex items-center justify-between">
      <div className="flex items-center">
        <span className="text-xs font-medium text-[var(--text-subtext)]">Background Services</span>
        <StudioTooltip
          section="[container.services]"
          title='redis = "redis-server --save 60 1"'
          description="Background processes the guest keeps alive — databases, queues, dev servers. Restarted on failure, die with the container."
          quadlet="Environment=PODBOX_SERVICES_JSON=&quot;{...}&quot;"
        />
      </div>
      <button
        type="button"
        onClick={() => setServices([...services, { name: '', command: '', restart: 'on-failure' }])}
        className="text-xs text-[var(--accent-mauve)] hover:text-white flex items-center gap-1 cursor-pointer"
      >
        <Plus className="w-3 h-3" />
        <span>Add Service</span>
      </button>
    </div>

    <div className="space-y-2">
      {services.map((svc, idx) => (
        <div key={idx} className="flex items-center gap-2">
          <input
            type="text"
            value={svc.name}
            aria-label={`Service ${idx + 1} name`}
            onChange={(e) => {
              const updated = [...services];
              updated[idx].name = e.target.value;
              setServices(updated);
            }}
            placeholder="name (e.g. redis)"
            className={`${STUDIO_FIELD} w-32 shrink-0`}
          />
          <input
            type="text"
            value={svc.command}
            aria-label={`Service ${idx + 1} command`}
            onChange={(e) => {
              const updated = [...services];
              updated[idx].command = e.target.value;
              setServices(updated);
            }}
            placeholder="command"
            className={`${STUDIO_FIELD} flex-1`}
          />
          <select
            value={svc.restart || 'on-failure'}
            aria-label={`Service ${idx + 1} restart policy`}
            onChange={(e) => {
              const updated = [...services];
              updated[idx].restart = e.target.value;
              setServices(updated);
            }}
            className={`${STUDIO_FIELD} px-2 cursor-pointer`}
          >
            {SERVICE_RESTART_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
          <button
            type="button"
            onClick={() => setServices(services.filter((_, i) => i !== idx))}
            className="p-1.5 text-[var(--text-muted)] hover:text-[var(--accent-red)] rounded-[2px] cursor-pointer"
            title="Remove service"
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
