import React from 'react';
import {
  Radio,
} from 'lucide-react';
import {
  StudioSelect,
  StudioTagInput,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import type { StudioState } from '../useStudioState';

type DbusPanelProps = Pick<StudioState, 'dbusPreset' | 'dbusTalkList' | 'setDbusPreset' | 'setDbusTalkList'>;

export function DbusPanel({ st }: { st: DbusPanelProps }) {
  const { dbusPreset, dbusTalkList, setDbusPreset, setDbusTalkList } = st;
  return (
<div className="space-y-5 animate-fadeIn">
  <div className="pb-3 border-b border-[var(--border)]">
    <div className="flex items-center gap-2">
      <Radio className="w-4 h-4 text-[var(--accent-mauve)]" />
      <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
        [dbus] — xdg-dbus-proxy Filter Rules
      </h2>
    </div>
    <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
      Configure granular D-Bus session bus access rules for desktop portals, notifications, and media keys.
    </p>
  </div>

  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
    <StudioSelect
      label={
        <div className="flex items-center">
          <span>D-Bus Security Preset</span>
          <StudioTooltip
            section="[dbus]"
            title="preset = &quot;portal&quot;"
            description="Preconfigured bus filtering rules matching Flatpak sandbox expectations."
          />
        </div>
      }
      value={dbusPreset}
      onChange={setDbusPreset}
      options={[
        { value: 'portal', label: 'portal (Desktop Portals & Notifications)' },
        { value: 'gnome', label: 'gnome (GNOME Shell integration)' },
        { value: 'kde', label: 'kde (KDE Plasma integration)' },
        { value: 'flatpak', label: 'flatpak (Strict Flatpak compatibility)' },
        { value: 'none', label: 'none (Custom rules only)' },
      ]}
    />

    <StudioTagInput
      label={
        <div className="flex items-center">
          <span>Talk Bus Names (Allowed Messages)</span>
          <StudioTooltip
            section="[dbus]"
            title="talk = [&quot;org.freedesktop.Notifications&quot;]"
            description="D-Bus bus names the container is permitted to call methods on."
          />
        </div>
      }
      tags={dbusTalkList}
      onChange={setDbusTalkList}
      placeholder="e.g. org.freedesktop.Notifications"
    />
  </div>
</div>
  );
}
