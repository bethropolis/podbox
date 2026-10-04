import React from 'react';
import {
  RefreshCw,
} from 'lucide-react';
import {
  StudioInput,
  StudioSelect,
  StudioSwitch,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import { ON_STOP_OPTIONS } from '../schema';
import type { StudioState } from '../useStudioState';

type LifecyclePanelProps = Pick<StudioState, 'lifeAutoCheckpoint' | 'lifeAutoUpdate' | 'lifeAutostart' | 'lifeOnStop' | 'lifeQuadlet' | 'setLifeAutoCheckpoint' | 'setLifeAutoUpdate' | 'setLifeAutostart' | 'setLifeOnStop' | 'setLifeQuadlet' | 'setSysAfter' | 'sysAfter'>;

export function LifecyclePanel({ st }: { st: LifecyclePanelProps }) {
  const { lifeAutoCheckpoint, lifeAutoUpdate, lifeAutostart, lifeOnStop, lifeQuadlet, setLifeAutoCheckpoint, setLifeAutoUpdate, setLifeAutostart, setLifeOnStop, setLifeQuadlet, setSysAfter, sysAfter } = st;
  return (
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
        options={ON_STOP_OPTIONS}
      />
    </div>

    <StudioSwitch
      id="life-autocheckpoint"
      checked={lifeAutoCheckpoint}
      onChange={setLifeAutoCheckpoint}
      label={
        <div className="flex items-center">
          <span>Auto Checkpoint</span>
          <StudioTooltip
            section="[lifecycle]"
            title="auto_checkpoint = true"
            description="Checkpoint the container to disk on stop for faster restores."
          />
        </div>
      }
      description="Persist container state on stop"
    />
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
  );
}
