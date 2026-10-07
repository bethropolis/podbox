import React from 'react';
import {
  Plus,
  ShieldCheck,
  Trash2,
} from 'lucide-react';
import {
  StudioInput,
  StudioSelect,
  StudioSwitch,
  StudioTagInput,
  STUDIO_FIELD,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import type { SecretItem } from '../types';
import {
  CAP_PRESET_OPTIONS,
  SECRET_SOURCE_OPTIONS,
  SECRET_TYPE_OPTIONS,
  USERNS_OPTIONS,
} from '../schema';
import type { StudioState } from '../useStudioState';

type SecurityPanelProps = Pick<StudioState, 'apparmor' | 'capPreset' | 'extraCapAddList' | 'noNewPrivileges' | 'readOnlyRootfs' | 'secLabelDisable' | 'seccomp' | 'secrets' | 'setApparmor' | 'setCapPreset' | 'setExtraCapAddList' | 'setNoNewPrivileges' | 'setReadOnlyRootfs' | 'setSecLabelDisable' | 'setSeccomp' | 'setSecrets' | 'setUsernsMode' | 'usernsMode'>;

export function SecurityPanel({ st }: { st: SecurityPanelProps }) {
  const { apparmor, capPreset, extraCapAddList, secrets, noNewPrivileges, readOnlyRootfs, seccomp, secLabelDisable, setApparmor, setCapPreset, setExtraCapAddList, setSecrets, setNoNewPrivileges, setReadOnlyRootfs, setSecLabelDisable, setSeccomp, setUsernsMode, usernsMode } = st;
  return (
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
        options={USERNS_OPTIONS}
      />
    </div>
  </div>

  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-2 border-t border-[var(--border)]">
    <StudioInput
      label={
        <div className="flex items-center">
          <span>AppArmor Profile</span>
          <StudioTooltip
            section="[security]"
            title="apparmor = &quot;...&quot;"
            description="AppArmor profile confining the container. Empty means podman default."
          />
        </div>
      }
      value={apparmor}
      onChange={(e) => setApparmor(e.target.value)}
      placeholder="podman default"
      id="studio-input-security-apparmor"
    />

    <StudioInput
      label={
        <div className="flex items-center">
          <span>Seccomp Profile</span>
          <StudioTooltip
            section="[security]"
            title="seccomp = &quot;...&quot;"
            description="Seccomp filter profile path or `unconfined`. Empty means podman default."
          />
        </div>
      }
      value={seccomp === 'default' ? '' : seccomp}
      onChange={(e) => setSeccomp(e.target.value || 'default')}
      placeholder="podman default"
      id="studio-input-security-seccomp"
    />
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
      options={CAP_PRESET_OPTIONS}
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

  {/* Podman / systemd secrets */}
  <div className="space-y-3 pt-2 border-t border-[var(--border)]">
    <div className="flex items-center justify-between">
      <div className="flex items-center">
        <span className="text-xs font-medium text-[var(--text-subtext)]">Secrets</span>
        <StudioTooltip
          section="[security].secrets"
          title='secrets = ["openai_key"]'
          description="Hand the container a value that must never be baked into the image or typed on a command line. A bare name reads a `podman secret` and exposes it as an environment variable; switch the type to mount it as a file instead, or take it from a systemd credential."
          quadlet="Secret=openai_key,type=env,target=openai_key"
        />
      </div>
      <button
        type="button"
        onClick={() =>
          setSecrets([...secrets, { name: '', secretType: 'env', target: '', mode: '', source: 'podman' }])
        }
        className="text-xs text-[var(--accent-mauve)] hover:text-white flex items-center gap-1 cursor-pointer"
      >
        <Plus className="w-3 h-3" />
        <span>Add Secret</span>
      </button>
    </div>

    <div className="space-y-2">
      {secrets.map((sec, idx) => {
        const update = (patch: Partial<SecretItem>) => {
          const next = [...secrets];
          next[idx] = { ...next[idx], ...patch };
          setSecrets(next);
        };
        return (
          <div key={idx} className="space-y-2 rounded-[2px] border border-[var(--border)] bg-[var(--bg-mantle)] p-2">
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={sec.name}
                aria-label={`Secret ${idx + 1} name`}
                spellCheck={false}
                onChange={(e) => update({ name: e.target.value })}
                placeholder="podman secret name"
                className={`${STUDIO_FIELD} flex-1`}
              />
              <button
                type="button"
                onClick={() => setSecrets(secrets.filter((_, i) => i !== idx))}
                className="p-1.5 text-[var(--text-muted)] hover:text-[var(--accent-red)] rounded-[2px] cursor-pointer shrink-0"
                title={`Remove ${sec.name || 'secret'}`}
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            </div>
            {/* A bare entry is emitted as `secrets = ["name"]`; these options
                are what switches it to the detailed form. */}
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
              <StudioSelect
                value={sec.secretType}
                onChange={(v) => update({ secretType: v as SecretItem['secretType'] })}
                options={SECRET_TYPE_OPTIONS}
              />
              <StudioSelect
                value={sec.source}
                onChange={(v) => update({ source: v as SecretItem['source'] })}
                options={SECRET_SOURCE_OPTIONS}
              />
              <input
                type="text"
                value={sec.target}
                aria-label={`Secret ${idx + 1} target`}
                spellCheck={false}
                onChange={(e) => update({ target: e.target.value })}
                placeholder="target (default: name)"
                className={`${STUDIO_FIELD} px-2 py-1.5 text-xs`}
              />
            </div>
            {sec.secretType === 'mount' && (
              <input
                type="text"
                value={sec.mode}
                aria-label={`Secret ${idx + 1} mode`}
                spellCheck={false}
                onChange={(e) => update({ mode: e.target.value })}
                placeholder="mode (e.g. 0400)"
                className={`${STUDIO_FIELD} w-32`}
              />
            )}
          </div>
        );
      })}
      {secrets.length === 0 && (
        <p className="text-[11px] text-[var(--text-muted)] italic">
          No secrets — API keys must be baked into the image or passed by hand.
        </p>
      )}
    </div>
  </div>
</div>
  );
}
