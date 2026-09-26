import React, { useState } from 'react';
import { TerminalCodeBlock } from './TerminalCodeBlock';
import { StudioSwitch } from './StudioControls';
import { StudioTooltip } from './StudioTooltip';
import {
  Boxes,
  Cpu,
  Check,
  ArrowRight,
  ExternalLink,
  Sparkles,
  Monitor,
  Volume2,
  FolderSync,
  Radio,
  SlidersHorizontal,
  FileCode2,
  Terminal
} from 'lucide-react';

import { withBase } from '../base';
import { generatePlaygroundToml, generatePlaygroundQuadlet, type PlaygroundConfig } from './playground/generate';

interface ConfigPlaygroundProps {
  // Navigation is plain MPA links; no callback props cross the Astro boundary.
}

export function ConfigPlayground(_props: ConfigPlaygroundProps) {
  const [name, setName] = useState('dev-box');
  const [distro, setDistro] = useState<'fedora' | 'arch' | 'ubuntu' | 'debian' | 'custom'>('fedora');
  const [customImage, setCustomImage] = useState('ghcr.io/username/custom-env:latest');
  const [wayland, setWayland] = useState(true);
  const [pipewire, setPipewire] = useState(true);
  const [gpu, setGpu] = useState<'auto' | 'nvidia' | 'off'>('auto');
  const [dbusNotifications, setDbusNotifications] = useState(true);
  const [shareProjects, setShareProjects] = useState(true);
  const [packages, setPackages] = useState('neovim, ripgrep, git, fish');
  const [activeView, setActiveView] = useState<'toml' | 'quadlet'>('toml');

  const config: PlaygroundConfig = { name, distro, customImage, wayland, pipewire, gpu, dbusNotifications, shareProjects, packages };

  return (
    <div className="w-full bg-[var(--bg-mantle)] border border-[var(--border)] rounded-[3px] overflow-hidden shadow-lg font-sans">
      {/* Chrome Header Bar */}
      <div className="flex items-center justify-between px-4 py-2.5 bg-[var(--bg-crust)] border-b border-[var(--border)]">
        <div className="flex items-center gap-2">
          <div className="flex gap-1.5">
            <span className="w-2.5 h-2.5 rounded-full bg-[var(--accent-red)]/70 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-[var(--accent-yellow)]/70 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-[var(--accent-green)]/70 inline-block" />
          </div>
          <span className="text-xs font-mono text-[var(--text-subtext)] ml-2 flex items-center gap-1.5">
            <SlidersHorizontal className="w-3.5 h-3.5 text-[var(--accent-mauve)]" />
            <span>quick-config // podbox.toml</span>
          </span>
        </div>

        <a
          href={withBase('/studio')}
          className="flex items-center gap-1.5 text-xs text-[var(--accent-mauve)] hover:text-white font-medium cursor-pointer transition-colors"
        >
          <Sparkles className="w-3.5 h-3.5" />
          <span>Open Full Studio</span>
          <ArrowRight className="w-3.5 h-3.5" />
        </a>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-12 divide-y lg:divide-y-0 lg:divide-x divide-[var(--border)]">
        {/* Controls Column (5 cols) */}
        <div className="lg:col-span-5 p-4 sm:p-5 space-y-4 text-xs">
          <div className="space-y-1.5">
            <div className="flex items-center">
              <span className="text-[var(--text-subtext)] font-medium">Container Name</span>
              <StudioTooltip
                title="name = &quot;dev-box&quot;"
                description="Identifier for container and Quadlet unit file."
              />
            </div>
            <input
              type="text"
              value={name}
              aria-label="Container name"
              onChange={(e) =>
                setName(e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, ''))
              }
              className="w-full px-3 py-1.5 bg-[var(--bg-base)] border border-[var(--border)] rounded-[2px] font-mono text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent-mauve)] focus:ring-1 focus:ring-[var(--accent-mauve)]/30 transition-all"
              placeholder="e.g. dev-box"
            />
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center">
              <span className="text-[var(--text-subtext)] font-medium">Base Distribution</span>
              <StudioTooltip
                title="base = &quot;fedora:44&quot;"
                description="Verified upstream OCI base image."
              />
            </div>
            <div className="grid grid-cols-3 sm:grid-cols-5 gap-1.5">
              {(['fedora', 'arch', 'ubuntu', 'debian', 'custom'] as const).map((d) => (
                <button
                  key={d}
                  onClick={() => setDistro(d)}
                  type="button"
                  className={`px-2 py-1.5 rounded-[2px] border text-center cursor-pointer capitalize font-mono text-[11px] transition-all ${
                    distro === d
                      ? 'border-[var(--accent-mauve)] bg-[var(--accent-mauve)]/10 text-[var(--accent-mauve)] font-bold'
                      : 'border-[var(--border)] bg-[var(--bg-base)] text-[var(--text-subtext)] hover:border-[var(--border-focus)]'
                  }`}
                >
                  {d}
                </button>
              ))}
            </div>

            {distro === 'custom' && (
              <div className="pt-1.5">
                <input
                  type="text"
                  value={customImage}
                  aria-label="Custom image reference"
                  onChange={(e) => setCustomImage(e.target.value)}
                  placeholder="e.g. ghcr.io/org/custom:latest"
                  className="w-full px-3 py-1.5 bg-[var(--bg-base)] border border-[var(--border)] rounded-[2px] text-[var(--text-primary)] text-xs font-mono focus:outline-none focus:border-[var(--accent-mauve)]"
                />
              </div>
            )}
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center">
              <span className="text-[var(--text-subtext)] font-medium">Baked Packages</span>
              <StudioTooltip
                title="packages = [...]"
                description="Comma-separated package list baked into rootfs."
              />
            </div>
            <input
              type="text"
              value={packages}
              aria-label="Baked packages, comma-separated"
              onChange={(e) => setPackages(e.target.value)}
              className="w-full px-3 py-1.5 bg-[var(--bg-base)] border border-[var(--border)] rounded-[2px] font-mono text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent-mauve)] focus:ring-1 focus:ring-[var(--accent-mauve)]/30 transition-all"
            />
          </div>

          {/* Subsystems Switch Toggles */}
          <div className="space-y-2 pt-2 border-t border-[var(--border)]">
            <span className="text-[11px] text-[var(--text-muted)] uppercase tracking-wider font-mono font-bold block">
              Subsystems &amp; Isolation
            </span>

            <div className="space-y-2">
              <StudioSwitch
                id="play-wayland"
                checked={wayland}
                onChange={setWayland}
                label={
                  <div className="flex items-center">
                    <span>Wayland Display Socket</span>
                    <StudioTooltip
                      title="wayland = true"
                      description="Direct compositor passthrough for GUI tools."
                    />
                  </div>
                }
              />

              <StudioSwitch
                id="play-pipewire"
                checked={pipewire}
                onChange={setPipewire}
                label={
                  <div className="flex items-center">
                    <span>PipeWire Audio</span>
                    <StudioTooltip
                      title="audio = true"
                      description="Low latency host audio capture & playback."
                    />
                  </div>
                }
              />

              <StudioSwitch
                id="play-projects"
                checked={shareProjects}
                onChange={setShareProjects}
                label={
                  <div className="flex items-center">
                    <span>Mount ~/Projects</span>
                    <StudioTooltip
                      title="projects = true"
                      description="Mounts host ~/Projects with SELinux :z flag."
                    />
                  </div>
                }
              />

              <StudioSwitch
                id="play-dbus"
                checked={dbusNotifications}
                onChange={setDbusNotifications}
                label={
                  <div className="flex items-center">
                    <span>Desktop Notifications</span>
                    <StudioTooltip
                      title="dbus = true"
                      description="xdg-dbus-proxy filtered alerts."
                    />
                  </div>
                }
              />
            </div>
          </div>

          {/* GPU Acceleration Selector */}
          <div className="space-y-1.5 pt-1">
            <div className="flex items-center">
              <span className="text-[var(--text-subtext)] font-medium">GPU Acceleration</span>
              <StudioTooltip
                title="gpu = &quot;auto&quot;"
                description="DRI /dev/dri hardware rendering passthrough."
              />
            </div>
            <div className="grid grid-cols-3 gap-1.5">
              {(['auto', 'nvidia', 'off'] as const).map((g) => (
                <button
                  key={g}
                  onClick={() => setGpu(g)}
                  type="button"
                  className={`px-2 py-1.5 text-center font-mono rounded-[2px] border cursor-pointer uppercase text-[11px] transition-all ${
                    gpu === g
                      ? 'border-[var(--accent-blue)] bg-[var(--accent-blue)]/10 text-[var(--accent-blue)] font-bold'
                      : 'border-[var(--border)] bg-[var(--bg-base)] text-[var(--text-subtext)] hover:border-[var(--border-focus)]'
                  }`}
                >
                  {g}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* Live Output Column (7 cols) */}
        <div className="lg:col-span-7 bg-[var(--bg-base)] flex flex-col">
          {/* View Mode Switcher */}
          <div className="flex items-center justify-between px-4 py-2 border-b border-[var(--border)] bg-[var(--bg-crust)]">
            <div className="flex items-center gap-1 font-mono text-xs">
              <button
                type="button"
                onClick={() => setActiveView('toml')}
                className={`px-3 py-1.5 rounded-[2px] transition-colors cursor-pointer flex items-center gap-1.5 ${
                  activeView === 'toml'
                    ? 'bg-[var(--accent-mauve)] text-[var(--bg-crust)] font-bold'
                    : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]'
                }`}
              >
                <FileCode2 className="w-3.5 h-3.5" />
                <span>podbox.toml</span>
              </button>

              <button
                type="button"
                onClick={() => setActiveView('quadlet')}
                className={`px-3 py-1.5 rounded-[2px] transition-colors cursor-pointer flex items-center gap-1.5 ${
                  activeView === 'quadlet'
                    ? 'bg-[var(--accent-peach)] text-[var(--bg-crust)] font-bold'
                    : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]'
                }`}
              >
                <Terminal className="w-3.5 h-3.5" />
                <span>{name}.container</span>
              </button>
            </div>

            <span className="text-[11px] font-mono text-[var(--text-muted)]">
              {activeView === 'toml' ? 'Declarative' : 'Systemd Quadlet'}
            </span>
          </div>

          {/* Rendered Syntax Block */}
          <div className="flex-1 p-2">
            <TerminalCodeBlock
              code={activeView === 'toml' ? generatePlaygroundToml(config) : generatePlaygroundQuadlet(config)}
              language={activeView === 'toml' ? 'toml' : 'ini'}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
