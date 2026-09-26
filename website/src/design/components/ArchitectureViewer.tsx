import React, { useState } from 'react';
import { Hammer, Play, ZoomIn, ZoomOut, Maximize2, FileCode2, Cpu, ArrowRight } from 'lucide-react';
import { withBase } from '../base';

export function ArchitectureViewer() {
  const [activeTab, setActiveTab] = useState<'build' | 'runtime'>('build');
  const [zoom, setZoom] = useState(false);

  return (
    <div className="w-full my-6 rounded-[4px] border border-[var(--border)] bg-[var(--bg-mantle)] overflow-hidden">
      {/* Titlebar & Tab controls */}
      <div className="flex flex-wrap items-center justify-between px-4 py-2.5 bg-[var(--bg-crust)] border-b border-[var(--border)] gap-2">
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-1.5">
            <span className="w-2.5 h-2.5 rounded-full bg-[#f38ba8]/80 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-[#f9e2af]/80 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-[#a6e3a1]/80 inline-block" />
          </div>
          <span className="text-xs text-[var(--text-subtext)] font-semibold uppercase tracking-wider">
            arch // {activeTab === 'build' ? 'build-time.svg' : 'runtime-flow.svg'}
          </span>
        </div>

        <div className="flex items-center gap-2">
          {/* Tab switcher */}
          <div className="inline-flex rounded-[2px] p-0.5 bg-[var(--bg-base)] border border-[var(--border)]">
            <button
              onClick={() => setActiveTab('build')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1 text-xs font-mono rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'build'
                  ? 'bg-[var(--accent-mauve)] text-[#11111b] font-bold shadow-xs'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <Hammer className="w-3.5 h-3.5" />
              Build-time
            </button>
            <button
              onClick={() => setActiveTab('runtime')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1 text-xs font-mono rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'runtime'
                  ? 'bg-[var(--accent-blue)] text-[#11111b] font-bold shadow-xs'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <Play className="w-3.5 h-3.5" />
              Runtime
            </button>
          </div>

          <button
            onClick={() => setZoom(!zoom)}
            type="button"
            className="p-1.5 text-xs text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] rounded-[2px] border border-[var(--border)] cursor-pointer"
            title={zoom ? 'Reset zoom' : 'Expand diagram'}
          >
            {zoom ? <ZoomOut className="w-3.5 h-3.5" /> : <Maximize2 className="w-3.5 h-3.5" />}
          </button>
        </div>
      </div>

      {/* SVG Diagram Canvas */}
      <div
        className={`p-4 md:p-6 bg-[var(--bg-base)] flex items-center justify-center transition-all ${
          zoom ? 'overflow-x-auto min-h-[500px]' : 'max-h-[520px] overflow-hidden'
        }`}
      >
        <div className={`w-full ${zoom ? 'min-w-[850px]' : 'max-w-[820px]'}`}>
          {activeTab === 'build' ? (
            <img
              src={withBase('/assets/architecture-build.svg')}
              alt="podbox build-time architecture"
              className="w-full h-auto drop-shadow-sm select-none"
            />
          ) : (
            <img
              src={withBase('/assets/architecture-runtime.svg')}
              alt="podbox runtime architecture"
              className="w-full h-auto drop-shadow-sm select-none"
            />
          )}
        </div>
      </div>

      {/* Architectural Explanations Footer */}
      <div className="p-4 bg-[var(--bg-mantle)] border-t border-[var(--border)] text-xs font-mono">
        {activeTab === 'build' ? (
          <div className="space-y-2">
            <div className="flex items-center gap-2 text-[var(--accent-mauve)] font-semibold">
              <FileCode2 className="w-4 h-4" />
              <span>Phase 1: Code Generation & Quadlet Synthesis</span>
            </div>
            <p className="text-[var(--text-subtext)] leading-relaxed">
              `podbox build` reads your TOML specification, generates a deterministic multi-stage Containerfile with the embedded guest daemon, invokes `podman build`, and renders standard systemd Quadlet files (`~/.config/containers/systemd/&lt;name&gt;.container`). No manual unit crafting needed.
            </p>
            <div className="flex flex-wrap gap-2 pt-1 text-[11px]">
              <span className="px-2 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-green)] border border-[var(--border)]">
                Input: podbox.toml
              </span>
              <span className="text-[var(--text-muted)] self-center">→</span>
              <span className="px-2 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-blue)] border border-[var(--border)]">
                OCI Image (localhost/podbox-&lt;name&gt;)
              </span>
              <span className="text-[var(--text-muted)] self-center">→</span>
              <span className="px-2 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-peach)] border border-[var(--border)]">
                systemd Quadlet (.container unit)
              </span>
            </div>
          </div>
        ) : (
          <div className="space-y-2">
            <div className="flex items-center gap-2 text-[var(--accent-blue)] font-semibold">
              <Cpu className="w-4 h-4" />
              <span>Phase 2: systemd Lifecycle & Interceptor Routing</span>
            </div>
            <p className="text-[var(--text-subtext)] leading-relaxed">
              systemd owns the container lifecycle via user units. Inside the container, a lightweight guest daemon intercepts notifications, clipboard requests, and host commands, routing them through a UNIX domain socket and `xdg-dbus-proxy` rather than granting unrestricted host bus access.
            </p>
            <div className="flex flex-wrap gap-2 pt-1 text-[11px]">
              <span className="px-2 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-mauve)] border border-[var(--border)]">
                systemctl --user start &lt;name&gt;
              </span>
              <span className="text-[var(--text-muted)] self-center">→</span>
              <span className="px-2 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-teal)] border border-[var(--border)]">
                xdg-dbus-proxy (Strict Rules)
              </span>
              <span className="text-[var(--text-muted)] self-center">→</span>
              <span className="px-2 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-green)] border border-[var(--border)]">
                Wayland / PipeWire Passthrough
              </span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
