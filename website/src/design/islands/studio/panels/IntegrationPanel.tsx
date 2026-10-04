import React from 'react';
import {
  Monitor,
  Plus,
  Trash2,
} from 'lucide-react';
import {
  StudioSelect,
  StudioSwitch,
  StudioTagInput,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import { GPU_OPTIONS } from '../schema';
import type { StudioState } from '../useStudioState';

type IntegrationPanelProps = Pick<StudioState, 'exportAppsList' | 'exportBinsList' | 'hostExecEnabled' | 'hostExecList' | 'intAudio' | 'intClipboard' | 'intDbus' | 'intGitIdentity' | 'intGpgAgent' | 'intGpu' | 'intNotify' | 'intSshAgent' | 'intSyncFonts' | 'intSyncIcons' | 'intSyncThemes' | 'intWayland' | 'intXdgOpen' | 'setExportAppsList' | 'setExportBinsList' | 'setHostExecEnabled' | 'setHostExecList' | 'setIntAudio' | 'setIntClipboard' | 'setIntDbus' | 'setIntGitIdentity' | 'setIntGpgAgent' | 'setIntGpu' | 'setIntNotify' | 'setIntSshAgent' | 'setIntSyncFonts' | 'setIntSyncIcons' | 'setIntSyncThemes' | 'setIntWayland' | 'setIntXdgOpen' | 'setXdgDesktop' | 'setXdgDocuments' | 'setXdgDownloads' | 'setXdgMusic' | 'setXdgPictures' | 'setXdgProjects' | 'setXdgVideos' | 'xdgDesktop' | 'xdgDocuments' | 'xdgDownloads' | 'xdgMusic' | 'xdgPictures' | 'xdgProjects' | 'xdgVideos'>;

export function IntegrationPanel({ st }: { st: IntegrationPanelProps }) {
  const { exportAppsList, exportBinsList, hostExecEnabled, hostExecList, intAudio, intClipboard, intDbus, intGitIdentity, intGpgAgent, intGpu, intNotify, intSshAgent, intSyncFonts, intSyncIcons, intSyncThemes, intWayland, intXdgOpen, setExportAppsList, setExportBinsList, setHostExecEnabled, setHostExecList, setIntAudio, setIntClipboard, setIntDbus, setIntGitIdentity, setIntGpgAgent, setIntGpu, setIntNotify, setIntSshAgent, setIntSyncFonts, setIntSyncIcons, setIntSyncThemes, setIntWayland, setIntXdgOpen, setXdgDesktop, setXdgDocuments, setXdgDownloads, setXdgMusic, setXdgPictures, setXdgProjects, setXdgVideos, xdgDesktop, xdgDocuments, xdgDownloads, xdgMusic, xdgPictures, xdgProjects, xdgVideos } = st;
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
      options={GPU_OPTIONS}
    />
  </div>

  {/* Desktop sync + identity */}
  <div className="pt-2 border-t border-[var(--border)] space-y-3">
    <div className="flex items-center">
      <span className="text-xs font-medium text-[var(--text-subtext)]">Desktop Sync &amp; Identity</span>
    </div>
    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
      <StudioSwitch
        id="int-sync-themes"
        checked={intSyncThemes}
        onChange={setIntSyncThemes}
        label="Theme Sync (~/.themes)"
        description="Share host GTK/icon themes read-only"
      />
      <StudioSwitch
        id="int-sync-icons"
        checked={intSyncIcons}
        onChange={setIntSyncIcons}
        label="Icon Sync (~/.icons)"
        description="Share host icon themes read-only"
      />
      <StudioSwitch
        id="int-sync-fonts"
        checked={intSyncFonts}
        onChange={setIntSyncFonts}
        label="Font Sync (~/.fonts)"
        description="Share host fonts read-only"
      />
      <StudioSwitch
        id="int-git-identity"
        checked={intGitIdentity}
        onChange={setIntGitIdentity}
        label="Git Identity Passthrough"
        description="Mount gitconfig + credentials (git_identity)"
      />
      <StudioSwitch
        id="int-gpg-agent"
        checked={intGpgAgent}
        onChange={setIntGpgAgent}
        label="Forward GPG Agent"
        description="Mount S.gpg-agent for commit signing"
      />
      <StudioSwitch
        id="int-xdg-open"
        checked={intXdgOpen}
        onChange={setIntXdgOpen}
        label="XDG Open Portal"
        description="Allow opening host URLs/files (xdg_open)"
      />
    </div>
  </div>

  {/* XDG directories */}
  <div className="pt-2 border-t border-[var(--border)] space-y-3">
    <div className="flex items-center">
      <span className="text-xs font-medium text-[var(--text-subtext)]">XDG Directories</span>
      <StudioTooltip
        section="[integration.xdg_dirs]"
        title="documents = true"
        description="Bind-mount host XDG user directories into the container."
      />
    </div>
    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
      <StudioSwitch id="xdg-documents" checked={xdgDocuments} onChange={setXdgDocuments} label="Documents" />
      <StudioSwitch id="xdg-downloads" checked={xdgDownloads} onChange={setXdgDownloads} label="Downloads" />
      <StudioSwitch id="xdg-pictures" checked={xdgPictures} onChange={setXdgPictures} label="Pictures" />
      <StudioSwitch id="xdg-music" checked={xdgMusic} onChange={setXdgMusic} label="Music" />
      <StudioSwitch id="xdg-videos" checked={xdgVideos} onChange={setXdgVideos} label="Videos" />
      <StudioSwitch id="xdg-desktop" checked={xdgDesktop} onChange={setXdgDesktop} label="Desktop" />
      <StudioSwitch id="xdg-projects" checked={xdgProjects} onChange={setXdgProjects} label="Projects" />
    </div>
  </div>

  {/* Host exec */}
  <div className="pt-2 border-t border-[var(--border)] space-y-3">
    <StudioSwitch
      id="host-exec-enabled"
      checked={hostExecEnabled}
      onChange={setHostExecEnabled}
      label={
        <div className="flex items-center">
          <span>Host Command Execution</span>
          <StudioTooltip
            section="[integration.host_exec]"
            title="enabled = true"
            description="Allowlist host binaries the container may invoke (filtered shims)."
          />
        </div>
      }
      description="Expose allowlisted host commands inside the container"
    />
    {hostExecEnabled && (
      <div className="space-y-2">
        {hostExecList.map((e, idx) => (
          <div key={idx} className="flex items-center gap-2">
            <input
              type="text"
              value={e.alias}
              aria-label={`Host exec ${idx + 1} alias`}
              onChange={(ev) => {
                const updated = [...hostExecList];
                updated[idx].alias = ev.target.value;
                setHostExecList(updated);
              }}
              placeholder="alias"
              className="w-32 px-2.5 py-1.5 text-xs font-mono rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent-mauve)]"
            />
            <input
              type="text"
              value={e.path}
              aria-label={`Host exec ${idx + 1} path`}
              onChange={(ev) => {
                const updated = [...hostExecList];
                updated[idx].path = ev.target.value;
                setHostExecList(updated);
              }}
              placeholder="/usr/bin/..."
              className="flex-1 px-2.5 py-1.5 text-xs font-mono rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent-mauve)]"
            />
            <button
              type="button"
              onClick={() => setHostExecList(hostExecList.filter((_, i) => i !== idx))}
              className="p-1.5 text-[var(--text-muted)] hover:text-[var(--accent-red)] rounded-[2px] cursor-pointer"
              title="Remove entry"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          </div>
        ))}
        <button
          type="button"
          onClick={() => setHostExecList([...hostExecList, { alias: '', path: '' }])}
          className="text-xs text-[var(--accent-mauve)] hover:text-white flex items-center gap-1 cursor-pointer"
        >
          <Plus className="w-3 h-3" />
          <span>Add Allowlist Entry</span>
        </button>
      </div>
    )}
  </div>

  {/* Desktop export */}
  <div className="pt-2 border-t border-[var(--border)] space-y-3">
    <div className="flex items-center">
      <span className="text-xs font-medium text-[var(--text-subtext)]">Desktop Export</span>
      <StudioTooltip
        section="[integration.export]"
        title="apps = [...]"
        description="Export container apps/bins to the host desktop."
      />
    </div>
    <StudioTagInput
      label="Exported Apps"
      tags={exportAppsList}
      onChange={setExportAppsList}
      placeholder="e.g. firefox..."
    />
    <StudioTagInput
      label="Exported Bins"
      tags={exportBinsList}
      onChange={setExportBinsList}
      placeholder="e.g. code..."
    />
  </div>
</div>
  );
}
