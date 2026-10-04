import React from 'react';
import {
  ShieldCheck,
} from 'lucide-react';
import {
  StudioInput,
  StudioSelect,
  StudioSwitch,
  StudioTagInput,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import { CAP_PRESET_OPTIONS, USERNS_OPTIONS } from '../schema';
import type { StudioState } from '../useStudioState';

type SecurityPanelProps = Pick<StudioState, 'apparmor' | 'capPreset' | 'extraCapAddList' | 'noNewPrivileges' | 'readOnlyRootfs' | 'secLabelDisable' | 'seccomp' | 'setApparmor' | 'setCapPreset' | 'setExtraCapAddList' | 'setNoNewPrivileges' | 'setReadOnlyRootfs' | 'setSecLabelDisable' | 'setSeccomp' | 'setUsernsMode' | 'usernsMode'>;

export function SecurityPanel({ st }: { st: SecurityPanelProps }) {
  const { apparmor, capPreset, extraCapAddList, noNewPrivileges, readOnlyRootfs, seccomp, secLabelDisable, setApparmor, setCapPreset, setExtraCapAddList, setNoNewPrivileges, setReadOnlyRootfs, setSecLabelDisable, setSeccomp, setUsernsMode, usernsMode } = st;
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
</div>
  );
}
