import React from 'react';
import {
  Network,
} from 'lucide-react';
import {
  StudioSelect,
  StudioSwitch,
  StudioTagInput,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import { NETWORK_MODE_OPTIONS } from '../schema';
import type { StudioState } from '../useStudioState';

type NetworkPanelProps = Pick<StudioState, 'netMode' | 'netOffline' | 'portMappingsList' | 'setNetMode' | 'setNetOffline' | 'setPortMappingsList'>;

export function NetworkPanel({ st, errorMap }: { st: NetworkPanelProps; errorMap?: Record<string, string> }) {
  const { netMode, netOffline, portMappingsList, setNetMode, setNetOffline, setPortMappingsList } = st;
  const err = (field: string) => errorMap?.[field];
  return (
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
      options={NETWORK_MODE_OPTIONS}
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
      error={err('network.ports')}
    />
  </div>

  <div className="pt-2 border-t border-[var(--border)]">
    <StudioSwitch
      id="net-offline"
      checked={netOffline}
      onChange={setNetOffline}
      label={
        <div className="flex items-center">
          <span>Offline Mode</span>
          <StudioTooltip
            section="[network]"
            title="offline = true"
            description="Hard-disable all networking (overrides mode)."
          />
        </div>
      }
      description="Airgap the container completely"
    />
  </div>
</div>
  );
}
