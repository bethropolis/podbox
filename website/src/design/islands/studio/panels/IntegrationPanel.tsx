import React from 'react';
import {
  Monitor,
} from 'lucide-react';
import {
  StudioSelect,
  StudioSwitch,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import type { StudioState } from '../useStudioState';

type IntegrationPanelProps = Pick<StudioState, 'intAudio' | 'intClipboard' | 'intDbus' | 'intGpu' | 'intNotify' | 'intSshAgent' | 'intWayland' | 'setIntAudio' | 'setIntClipboard' | 'setIntDbus' | 'setIntGpu' | 'setIntNotify' | 'setIntSshAgent' | 'setIntWayland'>;

export function IntegrationPanel({ st }: { st: IntegrationPanelProps }) {
  const { intAudio, intClipboard, intDbus, intGpu, intNotify, intSshAgent, intWayland, setIntAudio, setIntClipboard, setIntDbus, setIntGpu, setIntNotify, setIntSshAgent, setIntWayland } = st;
  return (
<div className="space-y-5 animate-fadeIn">
  <div className="pb-3 border-b border-[var(--border)]">
    <div className="flex items-center gap-2">
      <Monitor className="w-4 h-4 text-[var(--accent-mauve)]" />
      <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
        [integration] — Wayland, Audio, GPU &amp; Desktop
      </h2>
    </div>
    <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
      Seamless Linux desktop passthrough: Wayland display socket, PipeWire audio, DRI GPU acceleration, and desktop sharing.
    </p>
  </div>

  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
    <StudioSwitch
      id="int-wayland"
      checked={intWayland}
      onChange={setIntWayland}
      label={
        <div className="flex items-center">
          <span>Wayland Display Socket</span>
          <StudioTooltip
            section="[integration]"
            title="wayland = true"
            description="Passes through WAYLAND_DISPLAY and mounts /run/user/1000/wayland-0 so GUI apps render natively on host compositor."
            quadlet="Volume=/run/user/%U/wayland-0:/run/user/1000/wayland-0:ro"
          />
        </div>
      }
      description="Run graphical Wayland applications on host compositor"
    />

    <StudioSwitch
      id="int-audio"
      checked={intAudio}
      onChange={setIntAudio}
      label={
        <div className="flex items-center">
          <span>PipeWire &amp; PulseAudio</span>
          <StudioTooltip
            section="[integration]"
            title="audio = true"
            description="Mounts /run/user/1000/pipewire-0 and pulse sockets for low-latency host audio playback and capture."
            quadlet="Volume=/run/user/%U/pipewire-0:/run/user/1000/pipewire-0:ro"
          />
        </div>
      }
      description="Full host audio playback and microphone capture"
    />

    <StudioSwitch
      id="int-dbus"
      checked={intDbus}
      onChange={setIntDbus}
      label={
        <div className="flex items-center">
          <span>Filtered D-Bus Proxy</span>
          <StudioTooltip
            section="[integration]"
            title="dbus = true"
            description="Spawns an xdg-dbus-proxy filtering access to notifications, portals, and media controls without exposing system bus."
          />
        </div>
      }
      description="xdg-dbus-proxy filtered session bus communication"
    />

    <StudioSwitch
      id="int-notify"
      checked={intNotify}
      onChange={setIntNotify}
      label={
        <div className="flex items-center">
          <span>Desktop Notifications</span>
          <StudioTooltip
            section="[integration]"
            title="notify = true"
            description="Allows container utilities (e.g. notify-send) to pop desktop notifications onto host screen."
          />
        </div>
      }
      description="Forward notify-send alerts to host notification server"
    />

    <StudioSwitch
      id="int-clipboard"
      checked={intClipboard}
      onChange={setIntClipboard}
      label={
        <div className="flex items-center">
          <span>Clipboard Sharing</span>
          <StudioTooltip
            section="[integration]"
            title="clipboard = true"
            description="Permits copy/paste synchronization between container terminal/apps and host desktop."
          />
        </div>
      }
      description="Seamless copy & paste between host and container"
    />

    <StudioSwitch
      id="int-ssh-agent"
      checked={intSshAgent}
      onChange={setIntSshAgent}
      label={
        <div className="flex items-center">
          <span>Forward SSH Agent</span>
          <StudioTooltip
            section="[integration]"
            title="ssh_agent = true"
            description="Mounts SSH_AUTH_SOCK into container so git operations can use host SSH keys without copying private keys."
          />
        </div>
      }
      description="Use host SSH keys for git clone/push securely"
    />
  </div>

  <div className="pt-2 border-t border-[var(--border)]">
    <StudioSelect
      label={
        <div className="flex items-center">
          <span>Hardware GPU Acceleration</span>
          <StudioTooltip
            section="[integration]"
            title="gpu = &quot;auto&quot; | &quot;nvidia&quot; | true | false"
            description="Direct rendering infrastructure (/dev/dri) passthrough for Vulkan, OpenGL, and compute workloads."
            quadlet="Device=/dev/dri"
          />
        </div>
      }
      value={intGpu}
      onChange={setIntGpu}
      options={[
        { value: 'auto', label: 'auto (Pass /dev/dri if present on host)' },
        { value: 'nvidia', label: 'nvidia (NVIDIA Container Toolkit CDI)' },
        { value: 'true', label: 'true (Require /dev/dri passthrough)' },
        { value: 'false', label: 'false (Software rendering only)' },
      ]}
    />
  </div>
</div>
  );
}
