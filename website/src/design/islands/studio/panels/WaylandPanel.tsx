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
      Intercept and filter dangerous Wayland globals (e.g., screencast/keylogger protocols) for enhanced GUI security.
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
          description="Filters wl_registry globals advertised by host compositor to block untrusted apps from taking screenshots or capturing keystrokes."
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
          description="Specific Wayland global interface strings hidden from container applications."
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
