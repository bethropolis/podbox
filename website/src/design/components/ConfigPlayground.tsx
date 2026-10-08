import React, { useState } from 'react';
import { TerminalCodeBlock } from './TerminalCodeBlock';
import { StudioSwitch } from './StudioControls';
import { StudioTooltip } from './StudioTooltip';
import {
  ArrowRight,
  Sparkles,
  SlidersHorizontal,
  FileCode2,
  Terminal
} from 'lucide-react';

import { withBase } from '../base';
import { generatePlaygroundToml, generatePlaygroundQuadlet, type PlaygroundConfig } from './playground/generate';

interface ConfigPlaygroundProps {
  // Navigation is plain MPA links; no callback props cross the Astro boundary.
}

const GPU_CHOICES = [
  { value: 'auto' as const, label: 'Auto' },
  { value: 'nvidia' as const, label: 'NVIDIA' },
  { value: 'off' as const, label: 'None' },
];

// One shared input style so the controls read as a single form rather than a
// pile of differently-weighted widgets.
const inputClass =
  'w-full px-3 py-2 bg-[var(--bg-base)] border border-[var(--border)] rounded-[2px] font-mono text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[var(--accent-mauve)] focus:ring-1 focus:ring-[var(--accent-mauve)]/30 transition-colors';

function groupLabel(children: React.ReactNode) {
  return (
    <span className="text-[11px] text-[var(--text-muted)] uppercase tracking-wider font-mono font-bold">
      {children}
    </span>
  );
}

function fieldLabel(label: string, title: string, description: string) {
  return (
    <div className="flex items-center">
      <span className="text-[var(--text-primary)] text-sm font-medium">{label}</span>
      <StudioTooltip title={title} description={description} />
    </div>
  );
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
  const code = activeView === 'toml' ? generatePlaygroundToml(config) : generatePlaygroundQuadlet(config);

  const tabs = [
    { id: 'toml' as const, label: 'podbox.toml', note: 'what you write', icon: FileCode2 },
    { id: 'quadlet' as const, label: `${name}.container`, note: 'what podbox runs', icon: Terminal },
  ];

  return (
    /* Two independent, top-aligned boxes rather than one shared frame. A
       shared frame forces both columns to the same height, which left the
       editor stretched around 20 lines of TOML with a large dead area under
       it; the output now hugs its code and follows the form down the page. */
    <div className="w-full grid grid-cols-1 lg:grid-cols-2 gap-5 items-start font-sans">
        {/* Controls */}
        <div className="border border-[var(--border)] rounded-[3px] bg-[var(--bg-mantle)] p-5 sm:p-7 space-y-7">
          <div className="space-y-5">
            {groupLabel('The basics')}
            <div className="h-px bg-[var(--border)]" />
          </div>

          <div className="space-y-2">
            {fieldLabel('Name', 'name = "dev-box"', 'Names the container, its home folder and its systemd unit.')}
            <input
              type="text"
              value={name}
              aria-label="Container name"
              onChange={(e) =>
                setName(e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, ''))
              }
              className={inputClass}
              placeholder="dev-box"
            />
            <p className="text-xs text-[var(--text-muted)] leading-relaxed">
              Used for the container, its folder and its systemd unit.
            </p>
          </div>

          <div className="space-y-2">
            {fieldLabel('Base image', 'base = "fedora:44"', 'The image your container starts from.')}
            <div className="grid grid-cols-3 sm:grid-cols-5 gap-1.5">
              {(['fedora', 'arch', 'ubuntu', 'debian', 'custom'] as const).map((d) => (
                <button
                  key={d}
                  onClick={() => setDistro(d)}
                  type="button"
                  aria-pressed={distro === d}
                  className={`px-2 py-2 rounded-[2px] border text-center cursor-pointer capitalize font-mono text-xs transition-colors ${
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
              <input
                type="text"
                value={customImage}
                aria-label="Custom image reference"
                onChange={(e) => setCustomImage(e.target.value)}
                placeholder="ghcr.io/org/custom:latest"
                className={inputClass}
              />
            )}
          </div>

          <div className="space-y-2">
            {fieldLabel('Packages to install', 'packages = [...]', 'Installed into the image at build time.')}
            <input
              type="text"
              value={packages}
              aria-label="Packages to install, comma-separated"
              onChange={(e) => setPackages(e.target.value)}
              className={inputClass}
            />
            <p className="text-xs text-[var(--text-muted)] leading-relaxed">
              Baked into the image, separated by commas.
            </p>
          </div>

          <div className="space-y-5">
            {groupLabel('What the container can use')}
            <div className="h-px bg-[var(--border)]" />
          </div>

          <div className="space-y-2.5">
            <StudioSwitch
              id="play-wayland"
              checked={wayland}
              onChange={setWayland}
              label={<span>Your display</span>}
              description="GUI apps draw straight to your screen."
            />

            <StudioSwitch
              id="play-pipewire"
              checked={pipewire}
              onChange={setPipewire}
              label={<span>Sound</span>}
              description="Plays and records audio through the host."
            />

            <StudioSwitch
              id="play-projects"
              checked={shareProjects}
              onChange={setShareProjects}
              label={<span>Your projects folder</span>}
              description="Mounts ~/Projects so your code is right there."
            />

            <StudioSwitch
              id="play-dbus"
              checked={dbusNotifications}
              onChange={setDbusNotifications}
              label={<span>Notifications</span>}
              description="Desktop alerts from inside the container."
            />
          </div>

          <div className="space-y-5">
            {groupLabel('Graphics')}
            <div className="h-px bg-[var(--border)]" />
          </div>

          <div className="space-y-2">
            {fieldLabel('GPU access', 'gpu = "auto"', 'Passes the graphics card through to the container.')}
            <div className="grid grid-cols-3 gap-1.5">
              {GPU_CHOICES.map(({ value, label }) => (
                <button
                  key={value}
                  onClick={() => setGpu(value)}
                  type="button"
                  aria-pressed={gpu === value}
                  className={`px-2 py-2 text-center rounded-[2px] border cursor-pointer text-xs transition-colors ${
                    gpu === value
                      ? 'border-[var(--accent-blue)] bg-[var(--accent-blue)]/10 text-[var(--accent-blue)] font-bold'
                      : 'border-[var(--border)] bg-[var(--bg-base)] text-[var(--text-subtext)] hover:border-[var(--border-focus)]'
                  }`}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* Output */}
        <div className="lg:sticky lg:top-20 lg:max-h-[calc(100vh-6rem)] border border-[var(--border)] rounded-[3px] bg-[var(--bg-base)] flex flex-col overflow-hidden">
          <div className="shrink-0 flex items-center justify-between gap-3 px-5 sm:px-6 py-3.5 border-b border-[var(--border)]">
            <div className="flex items-center gap-2 text-sm">
              <SlidersHorizontal className="w-4 h-4 text-[var(--accent-green)] shrink-0" />
              <span className="text-[var(--text-primary)] font-medium">Your config</span>
            </div>
            <a
              href={withBase('/studio')}
              className="flex items-center gap-1.5 text-xs text-[var(--accent-mauve)] hover:text-white font-medium cursor-pointer transition-colors whitespace-nowrap"
            >
              <Sparkles className="w-3.5 h-3.5" />
              <span>Open in Studio</span>
              <ArrowRight className="w-3.5 h-3.5" />
            </a>
          </div>

          {/* File tabs */}
          <div className="shrink-0 flex items-end gap-1 px-5 sm:px-6 pt-3 border-b border-[var(--border)]" role="tablist">
            {tabs.map(({ id, label, note, icon: Icon }) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={activeView === id}
                onClick={() => setActiveView(id)}
                className={`flex flex-col gap-0.5 px-3 py-2 -mb-px border-b-2 text-left cursor-pointer transition-colors ${
                  activeView === id
                    ? 'border-[var(--accent-mauve)]'
                    : 'border-transparent hover:bg-[var(--bg-mantle)]/60'
                }`}
              >
                <span
                  className={`flex items-center gap-1.5 font-mono text-xs ${
                    activeView === id ? 'text-[var(--accent-mauve)]' : 'text-[var(--text-subtext)]'
                  }`}
                >
                  <Icon className="w-3.5 h-3.5 shrink-0" />
                  {label}
                </span>
                <span className="text-[11px] text-[var(--text-muted)]">{note}</span>
              </button>
            ))}
          </div>

          {/* Code sits directly in the panel body, no window inside a window. It
              scrolls inside the panel when a long file would otherwise hang
              below a short viewport while the panel is pinned. */}
          <div className="flex-1 min-h-0 overflow-auto">
            <TerminalCodeBlock code={code} language={activeView === 'toml' ? 'toml' : 'ini'} bare />
          </div>
        </div>
    </div>
  );
}