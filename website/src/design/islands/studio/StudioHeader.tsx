import React, { useState } from 'react';
import {
  ArrowLeft,
  ChevronDown,
  Copy,
  Download,
  FileCode2,
  Flame,
  Globe,
  Lock,
  Maximize2,
  Minimize2,
  Monitor,
  RotateCcw,
  Save,
  Sparkles,
  Trash2,
  Upload,
} from 'lucide-react';
import { withBase } from '../../base';
import type { StudioState } from './useStudioState';
import { useOutputActions } from './useOutputActions';
import { presetPatch, defaultPatch } from './presets';
import { StudioImport } from './StudioImport';

interface StudioHeaderProps { st: Pick<StudioState, 'activePreset' | 'applyPatch' | 'containerName' | 'isFullscreen' | 'setIsFullscreen' | 'activeView' | 'copied' | 'setCopied' | 'setShowExportMenu' | 'showExportMenu' | 'hasRestoredSession' | 'clearSavedSession'>; toml: string; quadlet: string; containerfile: string | null; }

export function StudioHeader({ st, toml, quadlet, containerfile }: StudioHeaderProps) {
  const { activePreset, containerName, isFullscreen, setIsFullscreen, setShowExportMenu, showExportMenu, hasRestoredSession } = st;
  const { handleDownloadToml, handleCopyConfig } = useOutputActions(st, toml, quadlet, containerfile);
  const [showImport, setShowImport] = useState(false);
  const [showPresetMenu, setShowPresetMenu] = useState(false);
  // Fullscreen is a workbench, not a landing page: drop to a single slim
  // toolbar row with icon-only actions.
  const iconOnly = isFullscreen;
  const onPreset = (p: 'rust' | 'arch-gui' | 'fullstack' | 'minimal') => {
    st.applyPatch(presetPatch(p));
    setShowPresetMenu(false);
  };
  const onReset = () => st.applyPatch(defaultPatch());
  const onClearSession = () => st.clearSavedSession();
  return (
<div className={`shrink-0 border-b border-[var(--border)] ${isFullscreen ? 'pb-2.5 mb-2.5 space-y-2' : 'pb-4 mb-4 space-y-3'}`}>
  <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
    {/* Left: Branding & status */}
    <div className="flex items-center gap-3">
      {!isFullscreen && (
        <a
          href={withBase('/')}
          className="p-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] rounded-[2px] transition-colors cursor-pointer"
          title="Back to Overview"
        >
          <ArrowLeft className="w-4 h-4" />
        </a>
      )}
      <div>
        <h1 className={`font-bold tracking-tight text-[var(--text-primary)] ${iconOnly ? 'text-sm' : 'text-xl sm:text-2xl'}`}>
          podbox Studio
        </h1>
      </div>
    </div>

    {/* Right: Actions (Presets, Export, Reset, Fullscreen) */}
    <div className="flex flex-wrap items-center gap-2">
      {/* Autosave state */}
      <div
        className={`items-center gap-1.5 text-[11px] font-mono text-[var(--text-muted)] ${iconOnly ? 'hidden' : 'hidden lg:flex'}`}
        title={hasRestoredSession
          ? 'Your last session was restored from this browser'
          : 'Changes are saved to this browser automatically'}
      >
        <Save className="w-3.5 h-3.5" />
        <span>{hasRestoredSession ? 'session restored' : 'autosaved locally'}</span>
      </div>

      {/* Reset button */}
      <button
        onClick={onReset}
        type="button"
        className="px-2.5 py-1.5 rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-xs text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:border-[var(--accent-red)]/50 transition-colors cursor-pointer flex items-center gap-1.5"
        title="Reset all settings to default"
      >
        <RotateCcw className="w-3.5 h-3.5 text-[var(--text-muted)]" />
        {!iconOnly && <span className="hidden sm:inline">Reset</span>}
      </button>

      {/* Presets dropdown */}
      <div className="relative">
        <button
          onClick={() => setShowPresetMenu(!showPresetMenu)}
          type="button"
          className="px-2.5 py-1.5 rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-xs text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:border-[var(--accent-mauve)]/50 transition-colors cursor-pointer flex items-center gap-1.5"
          title="Apply a curated preset"
        >
          <Sparkles className="w-3.5 h-3.5 text-[var(--text-muted)]" />
          {!iconOnly && <span className="hidden sm:inline">Presets</span>}
          <ChevronDown className="w-3.5 h-3.5 ml-0.5" />
        </button>

        {showPresetMenu && (
          <>
            <div
              className="fixed inset-0 z-40"
              onClick={() => setShowPresetMenu(false)}
            />
            <div className="absolute left-0 top-full mt-1.5 z-50 w-52 rounded-[3px] bg-[var(--bg-mantle)] border border-[var(--border)] shadow-2xl p-1 text-xs font-sans animate-fadeIn">
              {([
                { id: 'rust', label: 'Rust Dev', Icon: Flame, iconColor: 'text-[var(--accent-peach)]' },
                { id: 'arch-gui', label: 'Arch GUI', Icon: Monitor, iconColor: 'text-[var(--accent-blue)]' },
                { id: 'fullstack', label: 'Full-Stack', Icon: Globe, iconColor: 'text-[var(--accent-green)]' },
                { id: 'minimal', label: 'Hardened', Icon: Lock, iconColor: 'text-[var(--accent-red)]' },
              ] as const).map(({ id, label, Icon, iconColor }) => (
                <button
                  key={id}
                  onClick={() => onPreset(id)}
                  className={`w-full text-left px-3 py-2 rounded-[2px] hover:bg-[var(--bg-surface0)] flex items-center gap-2 cursor-pointer ${
                    activePreset === id
                      ? 'text-[var(--accent-mauve)] font-bold'
                      : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
                  }`}
                >
                  <Icon className={`w-4 h-4 ${iconColor}`} />
                  <span>{label}</span>
                  {activePreset === id && <span className="ml-auto">✓</span>}
                </button>
              ))}
            </div>
          </>
        )}
      </div>

      {/* Import button */}
      <button
        onClick={() => setShowImport(true)}
        type="button"
        className="px-2.5 py-1.5 rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-xs text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:border-[var(--accent-mauve)]/50 transition-colors cursor-pointer flex items-center gap-1.5"
        title="Import an existing podbox.toml"
      >
        <Upload className="w-3.5 h-3.5 text-[var(--text-muted)]" />
        {!iconOnly && <span className="hidden sm:inline">Import</span>}
      </button>

      {/* Export Dropdown */}
      <div className="relative">
        <button
          onClick={() => setShowExportMenu(!showExportMenu)}
          type="button"
          className="px-3 py-1.5 rounded-[2px] bg-[var(--accent-mauve)] text-[var(--bg-crust)] font-semibold text-xs transition-colors cursor-pointer flex items-center gap-1.5 shadow-sm hover:opacity-95"
        >
          <Download className="w-3.5 h-3.5" />
          {!iconOnly && <span>Export</span>}
          <ChevronDown className="w-3.5 h-3.5 ml-0.5" />
        </button>

        {showExportMenu && (
          <>
            <div
              className="fixed inset-0 z-40"
              onClick={() => setShowExportMenu(false)}
            />
            <div className="absolute right-0 top-full mt-1.5 z-50 w-64 rounded-[3px] bg-[var(--bg-mantle)] border border-[var(--border)] shadow-2xl p-1 text-xs font-sans animate-fadeIn">
              <button
                onClick={handleDownloadToml}
                className="w-full text-left px-3 py-2 rounded-[2px] hover:bg-[var(--bg-surface0)] text-[var(--text-primary)] flex items-center justify-between cursor-pointer"
              >
                <div className="flex items-center gap-2">
                  <FileCode2 className="w-4 h-4 text-[var(--accent-mauve)]" />
                  <div>
                    <div className="font-medium">Download podbox.toml</div>
                    <div className="text-[10px] text-[var(--text-muted)] font-mono">
                      {containerName || 'podbox'}.toml
                    </div>
                  </div>
                </div>
              </button>

              <div className="my-1 border-t border-[var(--border)]" />

              <button
                onClick={() => {
                  handleCopyConfig();
                  setShowExportMenu(false);
                }}
                className="w-full text-left px-3 py-2 rounded-[2px] hover:bg-[var(--bg-surface0)] text-[var(--text-subtext)] hover:text-[var(--text-primary)] flex items-center gap-2 cursor-pointer"
              >
                <Copy className="w-4 h-4 text-[var(--accent-blue)]" />
                <span>Copy Current Output</span>
              </button>

              <div className="my-1 border-t border-[var(--border)]" />

              <button
                onClick={() => {
                  onClearSession();
                  setShowExportMenu(false);
                }}
                className="w-full text-left px-3 py-2 rounded-[2px] hover:bg-[var(--bg-surface0)] text-[var(--text-subtext)] hover:text-[var(--accent-red)] flex items-center gap-2 cursor-pointer"
              >
                <Trash2 className="w-4 h-4" />
                <span>Forget Saved Session</span>
              </button>
            </div>
          </>
        )}
      </div>

      {/* Fullscreen toggle */}
      <button
        onClick={() => setIsFullscreen(!isFullscreen)}
        type="button"
        className="px-2.5 py-1.5 rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-xs text-[var(--text-subtext)] hover:text-[var(--text-primary)] transition-colors cursor-pointer flex items-center gap-1.5"
        title={isFullscreen ? 'Exit Fullscreen (Esc)' : 'Expand to Fullscreen'}
      >
        {isFullscreen ? (
          <Minimize2 className="w-3.5 h-3.5 text-[var(--accent-mauve)]" />
        ) : (
          <>
            <Maximize2 className="w-3.5 h-3.5 text-[var(--text-muted)]" />
            <span className="hidden sm:inline">Fullscreen</span>
          </>
        )}
      </button>
    </div>
  </div>

  {showImport && <StudioImport st={st} onClose={() => setShowImport(false)} />}

</div>
  );
}
