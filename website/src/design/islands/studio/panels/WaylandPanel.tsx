import React from 'react';
import {
  Layers3,
} from 'lucide-react';
import {
  StudioSwitch,
  StudioTagInput,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import type { StudioState } from '../useStudioState';

type WaylandPanelProps = Pick<StudioState, 'setWaylandBlockedList' | 'setWaylandFirewall' | 'waylandBlockedList' | 'waylandFirewall'>;

export function WaylandPanel({ st }: { st: WaylandPanelProps }) {
  const { setWaylandBlockedList, setWaylandFirewall, waylandBlockedList, waylandFirewall } = st;
  return (
<div className="space-y-5 animate-fadeIn">
  <div className="pb-3 border-b border-[var(--border)]">
    <div className="flex items-center gap-2">
      <Layers3 className="w-4 h-4 text-[var(--accent-mauve)]" />
      <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
        [wayland] — Protocol Firewall
      </h2>
    </div>
    <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
      Block screen capture and keylogging protocols.
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
          description="Blocks screenshots and keylogging."
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
          description="Protocol names hidden from container apps."
        />
      </div>
    }
    tags={waylandBlockedList}
    onChange={setWaylandBlockedList}
    placeholder="e.g. zwlr_screencopy_manager_v1"
  />
</div>
  );
}
