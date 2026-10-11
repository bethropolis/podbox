import React from 'react';
import { DbusPanel } from './DbusPanel';
import { WaylandPanel } from './WaylandPanel';
import type { StudioState } from '../useStudioState';

type ProxiesPanelProps = Pick<
  StudioState,
  | 'dbusPreset'
  | 'dbusTalkList'
  | 'setDbusPreset'
  | 'setDbusTalkList'
  | 'intDbus'
  | 'setWaylandBlockedList'
  | 'setWaylandFirewall'
  | 'waylandBlockedList'
  | 'waylandFirewall'
  | 'intWayland'
>;

// D-Bus proxy rules and the Wayland protocol firewall are podbox's two
// per-interface filters, so they share one tab. Both children keep their own
// headers and input ids — error focus, hover highlight, and badges resolve
// through those, never through the tab id.
export function ProxiesPanel({ st }: { st: ProxiesPanelProps }) {
  return (
    <div className="space-y-10">
      <DbusPanel st={st} />
      <div className="h-px bg-[var(--border)]" />
      <WaylandPanel st={st} />
    </div>
  );
}
