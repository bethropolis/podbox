import React, { useMemo } from 'react';
import {
  Monitor,
  Plus,
  Trash2,
} from 'lucide-react';
import {
  StudioSelect,
  StudioSwitch,
  StudioTagInput,
  STUDIO_FIELD,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import {
  GPU_OPTIONS,
  HOST_EXEC_SUGGESTIONS,
  type XdgDirMode,
} from '../schema';
import type { StudioState } from '../useStudioState';

type IntegrationPanelProps = Pick<StudioState, 'exportAppsList' | 'exportBinsList' | 'hostExecEnabled' | 'hostExecList' | 'hwJoystick' | 'hwKvm' | 'hwSerial' | 'hwWebcam' | 'hwYubikey' | 'intAudio' | 'intClipboard' | 'intDbus' | 'intGitIdentity' | 'intGpgAgent' | 'intGpu' | 'intNotify' | 'intSshAgent' | 'intSyncFonts' | 'intSyncIcons' | 'intSyncThemes' | 'intWayland' | 'intXdgOpen' | 'packagesInstallList' | 'setExportAppsList' | 'setExportBinsList' | 'setHostExecEnabled' | 'setHostExecList' | 'setHwJoystick' | 'setHwKvm' | 'setHwSerial' | 'setHwWebcam' | 'setHwYubikey' | 'setIntAudio' | 'setIntClipboard' | 'setIntDbus' | 'setIntGitIdentity' | 'setIntGpgAgent' | 'setIntGpu' | 'setIntNotify' | 'setIntSshAgent' | 'setIntSyncFonts' | 'setIntSyncIcons' | 'setIntSyncThemes' | 'setIntWayland' | 'setIntXdgOpen' | 'setXdgDesktop' | 'setXdgDocuments' | 'setXdgDownloads' | 'setXdgMusic' | 'setXdgPictures' | 'setXdgProjects' | 'setXdgVideos' | 'xdgDesktop' | 'xdgDocuments' | 'xdgDownloads' | 'xdgMusic' | 'xdgPictures' | 'xdgProjects' | 'xdgVideos'>;

export function IntegrationPanel({ st, errorMap }: { st: IntegrationPanelProps; errorMap?: Record<string, string> }) {
  const { exportAppsList, exportBinsList, hostExecEnabled, hostExecList, hwJoystick, hwKvm, hwSerial, hwWebcam, hwYubikey, intAudio, packagesInstallList, intClipboard, intDbus, intGitIdentity, intGpgAgent, intGpu, intNotify, intSshAgent, intSyncFonts, intSyncIcons, intSyncThemes, intWayland, intXdgOpen, setExportAppsList, setExportBinsList, setHostExecEnabled, setHostExecList, setHwJoystick, setHwKvm, setHwSerial, setHwWebcam, setHwYubikey, setIntAudio, setIntClipboard, setIntDbus, setIntGitIdentity, setIntGpgAgent, setIntGpu, setIntNotify, setIntSshAgent, setIntSyncFonts, setIntSyncIcons, setIntSyncThemes, setIntWayland, setIntXdgOpen, setXdgDesktop, setXdgDocuments, setXdgDownloads, setXdgMusic, setXdgPictures, setXdgProjects, setXdgVideos, xdgDesktop, xdgDocuments, xdgDownloads, xdgMusic, xdgPictures, xdgProjects, xdgVideos } = st;
  const err = (field: string) => errorMap?.[field];
  const XDG_DIRS = [
    { id: 'documents', label: 'Documents', mode: xdgDocuments, setter: setXdgDocuments },
    { id: 'downloads', label: 'Downloads', mode: xdgDownloads, setter: setXdgDownloads },
    { id: 'pictures', label: 'Pictures', mode: xdgPictures, setter: setXdgPictures },
    { id: 'music', label: 'Music', mode: xdgMusic, setter: setXdgMusic },
    { id: 'videos', label: 'Videos', mode: xdgVideos, setter: setXdgVideos },
    { id: 'desktop', label: 'Desktop', mode: xdgDesktop, setter: setXdgDesktop },
    { id: 'projects', label: 'Projects', mode: xdgProjects, setter: setXdgProjects },
  ] as const;
  // off -> read-only -> read-write -> off. The engine defaults an enabled dir
  // to read-only (`ro,z`) and only honours a writable bind through the
  // detailed table form, so the mode has to be explicit in the TOML.
  const cycleXdg = (mode: XdgDirMode) =>
    mode === 'off' ? 'ro' : mode === 'ro' ? 'rw' : 'off';
  // What can actually be exported is what the image contains, and the Studio
  // already knows that: `[image] packages.install`. Suggest those first (in
  // the order you listed them), then fall back to the curated list for
  // anything you did not install through podbox.
  const imagePackages = useMemo(
    () => packagesInstallList.map((p) => p.trim()).filter(Boolean),
    [packagesInstallList],
  );
  // Suggestions are the packages actually installed into the image. A curated
  // list looked helpful but mostly offered names that aren't in the image,
  // which is worse than no suggestion at all.
  const suggestions = useMemo(
    () => [...new Set(imagePackages)],
    [imagePackages],
  );

  // An allowlist entry only counts once both halves are filled in; the Rust
  // validator rejects partial rows, so the hint keys off the same rule.
  const allowlistReady = hostExecList.some((e) => e.alias.trim() && e.path.trim());
  const addAllowlistEntry = (entry: { alias: string; path: string }) => {
    if (hostExecList.some((e) => e.alias === entry.alias)) return;
    setHostExecList([...hostExecList, entry]);
  };
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
      Share Wayland, audio, GPU, and desktop features with the host.
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
            description="GUI apps show up on your desktop."
            quadlet="Volume=/run/user/%U/wayland-0:/run/user/1000/wayland-0:ro"
          />
        </div>
      }
      description="GUI apps on your desktop"
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
            description="Sound in and out of the container."
            quadlet="Volume=/run/user/%U/pipewire-0:/run/user/1000/pipewire-0:ro"
          />
        </div>
      }
      description="Sound and microphone"
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
            description="Filtered access to notifications and portals — never the whole bus."
          />
        </div>
      }
      description="Filtered desktop messaging"
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
            description="notify-send pops up on your desktop."
          />
        </div>
      }
      description="Alerts on your desktop"
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
            description="Git uses your host SSH keys. Keys never enter the container."
          />
        </div>
      }
      description="Git with your host keys"
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
            description="GPU for games, Vulkan, and compute."
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
        label={
          <div className="flex items-center">
            <span>Theme Sync</span>
            <StudioTooltip
              section="[integration]"
              title="sync_themes = true"
              description="Container apps use your host theme."
              quadlet="Volume=%h/.themes:/home/user/.themes:ro"
            />
          </div>
        }
      />
      <StudioSwitch
        id="int-sync-icons"
        checked={intSyncIcons}
        onChange={setIntSyncIcons}
        label={
          <div className="flex items-center">
            <span>Icon Sync</span>
            <StudioTooltip
              section="[integration]"
              title="sync_icons = true"
              description="Icons match your desktop."
              quadlet="Volume=%h/.icons:/home/user/.icons:ro"
            />
          </div>
        }
      />
      <StudioSwitch
        id="int-sync-fonts"
        checked={intSyncFonts}
        onChange={setIntSyncFonts}
        label={
          <div className="flex items-center">
            <span>Font Sync</span>
            <StudioTooltip
              section="[integration]"
              title="sync_fonts = true"
              description="Documents and terminals use your installed fonts."
              quadlet="Volume=%h/.fonts:/home/user/.fonts:ro"
            />
          </div>
        }
      />
      <StudioSwitch
        id="int-git-identity"
        checked={intGitIdentity}
        onChange={setIntGitIdentity}
        label={
          <div className="flex items-center">
            <span>Git Identity Passthrough</span>
            <StudioTooltip
              section="[integration]"
              title="git_identity = true"
              description="Commits authored as you; pushes use your credentials."
              quadlet="Volume=%h/.gitconfig:/home/user/.gitconfig:ro"
            />
          </div>
        }
      />
      <StudioSwitch
        id="int-gpg-agent"
        checked={intGpgAgent}
        onChange={setIntGpgAgent}
        label={
          <div className="flex items-center">
            <span>Forward GPG Agent</span>
            <StudioTooltip
              section="[integration]"
              title="gpg_agent = true"
              description="Sign commits without importing keys."
            />
          </div>
        }
      />
      <StudioSwitch
        id="int-xdg-open"
        checked={intXdgOpen}
        onChange={setIntXdgOpen}
        label={
          <div className="flex items-center">
            <span>XDG Open Portal</span>
            <StudioTooltip
              section="[integration]"
              title="xdg_open = true"
              description="Links open in your host browser."
            />
          </div>
        }
      />
    </div>
  </div>

  {/* XDG directories */}
  <div className="pt-2 border-t border-[var(--border)] space-y-3">
    <div className="flex items-center">
      <span className="text-xs font-medium text-[var(--text-subtext)]">XDG Directories</span>
      <StudioTooltip
        section="[integration.xdg_dirs]"
        title='projects = { enabled = true, read_write = true }'
        description="Mount the matching host folder (Documents, Downloads, …) so saved files land in the same place. Click a directory to cycle: off, read-only, read-write."
        quadlet="Volume=%h/Documents:/home/user/Documents:z"
      />
    </div>
    <div className="flex flex-wrap gap-1.5">
      {XDG_DIRS.map(({ id, label, mode, setter }) => (
        <button
          key={id}
          type="button"
          onClick={() => setter(cycleXdg(mode))}
          aria-pressed={mode !== 'off'}
          title={
            mode === 'rw'
              ? `~/${label} is mounted read-write. Click for read-only.`
              : mode === 'ro'
                ? `~/${label} is mounted read-only. Click for read-write, again to disable.`
                : `~/${label} is not mounted. Click to mount it read-only.`
          }
          className={`px-2 py-1 rounded-[2px] text-[11px] font-mono transition-colors cursor-pointer border flex items-center gap-1 ${
            mode === 'rw'
              ? 'bg-[var(--accent-green)]/15 border-[var(--accent-green)]/50 text-[var(--accent-green)] font-bold'
              : mode === 'ro'
                ? 'bg-[var(--accent-blue)]/10 border-[var(--accent-blue)]/40 text-[var(--accent-blue)] font-bold'
                : 'bg-[var(--bg-mantle)] border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:border-[var(--border-focus)]'
          }`}
        >
          {label}
          {mode !== 'off' && (
            <span className="text-[9px] uppercase opacity-80">{mode}</span>
          )}
        </button>
      ))}
    </div>
    <p className="text-[11px] text-[var(--text-muted)]">
      Click a directory to cycle: off &rarr; read-only &rarr; read-write.
    </p>
  </div>

  {/* Host hardware passthrough */}
  <div className="pt-2 border-t border-[var(--border)] space-y-3">
    <div className="flex items-center">
      <span className="text-xs font-medium text-[var(--text-subtext)]">Hardware Passthrough</span>
      <StudioTooltip
        section="[integration.hardware]"
        title="kvm = true"
        description="Hand host devices (webcam, gamepad, YubiKey, /dev/kvm) to the container. A device the host lacks is skipped, never fatal."
        quadlet="AddDevice=-/dev/kvm"
      />
    </div>
    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
      <StudioSwitch
        id="hw-webcam"
        checked={hwWebcam}
        onChange={setHwWebcam}
        label={
          <div className="flex items-center">
            <span>Webcam</span>
            <StudioTooltip
              section="[integration.hardware]"
              title="webcam = true"
              description="Camera for Zoom, OBS, OpenCV (/dev/video*)."
            />
          </div>
        }
      />
      <StudioSwitch
        id="hw-joystick"
        checked={hwJoystick}
        onChange={setHwJoystick}
        label={
          <div className="flex items-center">
            <span>Joystick / Gamepad</span>
            <StudioTooltip
              section="[integration.hardware]"
              title="joystick = true"
              description="Gamepads and joysticks (/dev/input)."
            />
          </div>
        }
      />
      <StudioSwitch
        id="hw-yubikey"
        checked={hwYubikey}
        onChange={setHwYubikey}
        label={
          <div className="flex items-center">
            <span>YubiKey / Smartcard</span>
            <StudioTooltip
              section="[integration.hardware]"
              title="yubikey = true"
              description="YubiKey and smartcards for signing and 2FA."
            />
          </div>
        }
      />
      <StudioSwitch
        id="hw-serial"
        checked={hwSerial}
        onChange={setHwSerial}
        label={
          <div className="flex items-center">
            <span>Serial / USB MCU</span>
            <StudioTooltip
              section="[integration.hardware]"
              title="serial = true"
              description="Microcontrollers and serial adapters (/dev/ttyUSB*)."
            />
          </div>
        }
      />
      <StudioSwitch
        id="hw-kvm"
        checked={hwKvm}
        onChange={setHwKvm}
        label={
          <div className="flex items-center">
            <span>KVM</span>
            <StudioTooltip
              section="[integration.hardware]"
              title="kvm = true"
              description="Nested virtualization: emulators, nested VMs."
            />
          </div>
        }
      />
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
            description="Let the container run host commands you name below. Nothing is exposed until allowlisted — no blanket host access."
            quadlet="Environment=PODBOX_HOST_EXEC_ALLOWLIST=&quot;{...}&quot;"
          />
        </div>
      }
      description="Expose allowlisted host commands inside the container"
    />
    {/* The engine rejects enabled-without-allowlist, and rightly so. Keep the
        error, but say what fixes it instead of just refusing. */}
    {hostExecEnabled && !allowlistReady && (
      <div className="flex flex-col gap-2 rounded-[3px] border border-[var(--accent-yellow)]/40 bg-[var(--accent-yellow)]/5 p-2.5">
        <p className="text-[11px] text-[var(--text-subtext)] leading-snug">
          <span className="font-bold text-[var(--accent-yellow)]">Needs an allowlist.</span>{' '}
          Add at least one alias + path pair below to unblock it.
        </p>
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="text-[10px] uppercase tracking-tight text-[var(--text-muted)]">
            Add common:
          </span>
          {HOST_EXEC_SUGGESTIONS.map((s) => (
            <button
              key={s.alias}
              type="button"
              onClick={() => addAllowlistEntry(s)}
              disabled={hostExecList.some((e) => e.alias === s.alias)}
              title={s.path}
              className="px-1.5 py-0.5 rounded-[2px] text-[11px] font-mono border border-[var(--border)] bg-[var(--bg-mantle)] text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:border-[var(--accent-mauve)]/50 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-default disabled:hover:text-[var(--text-subtext)]"
            >
              {s.alias}
            </button>
          ))}
        </div>
      </div>
    )}
    {err('integration.host_exec') && !allowlistReady && (
      <p className="text-[11px] leading-snug text-[var(--text-muted)]">
        podbox will refuse this config until the allowlist has at least one entry.
      </p>
    )}
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
              className={`${STUDIO_FIELD} w-32 shrink-0`}
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
              className={`${STUDIO_FIELD} flex-1`}
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
        description="Container apps show up in your host launcher with icons (apps), or on your PATH as commands (bins). Both survive container stops."
      />
    </div>
    <StudioTagInput
      label="Exported Apps"
      tags={exportAppsList}
      onChange={setExportAppsList}
      placeholder="Pick an installed package or type a name..."
      suggestions={suggestions}
      helperText={
        imagePackages.length > 0
          ? `Suggests the ${imagePackages.length} package${imagePackages.length === 1 ? '' : 's'} you install. Anything can be typed.`
          : 'Add packages under [image] to get suggestions here.'
      }
    />
    <StudioTagInput
      label="Exported Bins"
      tags={exportBinsList}
      onChange={setExportBinsList}
      placeholder="Pick an installed package or type a binary..."
      suggestions={suggestions}
      helperText="Executables added to your PATH from inside the container."
    />
  </div>
</div>
  );
}
