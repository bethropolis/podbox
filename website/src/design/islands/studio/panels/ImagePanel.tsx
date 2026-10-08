import React from 'react';
import {
  Box,
  Plus,
  Trash2,
} from 'lucide-react';
import {
  StudioInput,
  StudioSelect,
  StudioTagInput,
  STUDIO_FIELD,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import { IMAGE_PRESET_OPTIONS, PACKAGE_MANAGER_OPTIONS } from '../schema';
import type { StudioState } from '../useStudioState';

type ImagePanelProps = Pick<StudioState, 'customImageBase' | 'imageType' | 'packageManager' | 'packagesInstallList' | 'packagesRemoveList' | 'pullRetry' | 'runCommands' | 'selectedPresetDistro' | 'setCustomImageBase' | 'setImageType' | 'setPackageManager' | 'setPackagesInstallList' | 'setPackagesRemoveList' | 'setPullRetry' | 'setRunCommands' | 'setSelectedPresetDistro'>;

export function ImagePanel({ st, errorMap }: { st: ImagePanelProps; errorMap?: Record<string, string> }) {
  const { customImageBase, imageType, packageManager, packagesInstallList, packagesRemoveList, pullRetry, runCommands, selectedPresetDistro, setCustomImageBase, setImageType, setPackageManager, setPackagesInstallList, setPackagesRemoveList, setPullRetry, setRunCommands, setSelectedPresetDistro } = st;
  const err = (field: string) => errorMap?.[field];
  return (
<div className="space-y-5 animate-fadeIn">
  <div className="pb-3 border-b border-[var(--border)]">
    <div className="flex items-center gap-2">
      <Box className="w-4 h-4 text-[var(--accent-mauve)]" />
      <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
        [image] — Base Distribution &amp; Packages
      </h2>
    </div>
    <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
      Define the base OS or image, packages baked into it, and extra build steps.
    </p>
  </div>

  {/* Base Image Selection Type */}
  <div className="space-y-2">
    <div className="flex items-center">
      <span className="text-xs font-medium text-[var(--text-subtext)]">Base OS Source</span>
      <StudioTooltip
        section="[image]"
        title="preset vs base"
        description="Tested distros, or any image URI."
        quadlet="FROM <base-image>"
      />
    </div>
    <div className="grid grid-cols-2 gap-2">
      <button
        type="button"
        onClick={() => setImageType('preset')}
        className={`px-3 py-2 text-xs font-mono rounded-[2px] border transition-all text-center cursor-pointer ${
          imageType === 'preset'
            ? 'bg-[var(--accent-mauve)]/10 border-[var(--accent-mauve)] text-[var(--accent-mauve)] font-semibold'
            : 'bg-[var(--bg-mantle)] border-[var(--border)] text-[var(--text-muted)] hover:border-[var(--border-focus)]'
        }`}
      >
        Curated Distribution
      </button>
      <button
        type="button"
        onClick={() => setImageType('custom')}
        className={`px-3 py-2 text-xs font-mono rounded-[2px] border transition-all text-center cursor-pointer ${
          imageType === 'custom'
            ? 'bg-[var(--accent-mauve)]/10 border-[var(--accent-mauve)] text-[var(--accent-mauve)] font-semibold'
            : 'bg-[var(--bg-mantle)] border-[var(--border)] text-[var(--text-muted)] hover:border-[var(--border-focus)]'
        }`}
      >
        Custom OCI Image
      </button>
    </div>
  </div>

  {imageType === 'preset' ? (
    <StudioSelect
      label={
        <div className="flex items-center">
          <span>Distribution Preset</span>
          <StudioTooltip
            section="[image]"
            title="preset = &quot;distro:tag&quot;"
            description="Package manager and user setup handled for you."
          />
        </div>
      }
      value={selectedPresetDistro}
      onChange={(val) => setSelectedPresetDistro(val)}
      options={IMAGE_PRESET_OPTIONS}
    />
  ) : (
    <StudioInput
      label={
        <div className="flex items-center">
          <span>Custom OCI Image URI</span>
          <StudioTooltip
            section="[image]"
            title="base = &quot;registry/org/image:tag&quot;"
            description="Full registry reference for your container base image."
          />
        </div>
      }
      value={customImageBase}
      onChange={(e) => setCustomImageBase(e.target.value)}
      placeholder="ghcr.io/org/custom-image:latest"
      id="studio-input-image-base"
      error={err('image.base')}
    />
  )}

  {/* Packages to install with Chip Tag Input */}
  <StudioTagInput
    label={
      <div className="flex items-center">
        <span>Packages to Install</span>
        <StudioTooltip
          section="[image]"
          title="packages = [&quot;pkg1&quot;, &quot;pkg2&quot;]"
          description="Baked into the image at build time."
        />
      </div>
    }
    tags={packagesInstallList}
    onChange={setPackagesInstallList}
    placeholder="Type package and press Enter..."
    helperText="Baked during podbox build into the immutable rootfs layer."
  />

  {/* Packages to remove */}
  <StudioTagInput
    label={
      <div className="flex items-center">
        <span>Packages to Remove (Optional)</span>
        <StudioTooltip
          section="[image]"
          title="remove_packages = [...]"
          description="Stock packages to strip out."
        />
      </div>
    }
    tags={packagesRemoveList}
    onChange={setPackagesRemoveList}
    placeholder="e.g. vim-minimal, nano..."
  />

  {/* Advanced Image tuning */}
  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
    <StudioSelect
      label={
        <div className="flex items-center">
          <span>Package Manager</span>
          <StudioTooltip
            section="[image]"
            title="package_manager = &quot;auto&quot;"
            description="Or let podbox detect it from the base image."
          />
        </div>
      }
      value={packageManager}
      onChange={setPackageManager}
      options={PACKAGE_MANAGER_OPTIONS}
    />

    <StudioInput
      label={
        <div className="flex items-center">
          <span>Pull Retries</span>
          <StudioTooltip
            section="[image]"
            title="pull_retry = 3"
            description="Pull retries on flaky networks."
          />
        </div>
      }
      type="number"
      value={pullRetry}
      onChange={(e) => setPullRetry(parseInt(e.target.value) || 0)}
      min={0}
      max={10}
    />
  </div>

  {/* Extra RUN commands — an ordered list, since order decides layer order */}
  <div className="space-y-2">
    <div className="flex items-center justify-between">
      <div className="flex items-center">
        <span className="text-xs font-medium text-[var(--text-subtext)]">Extra RUN Commands</span>
        <StudioTooltip
          section="[image.run]"
          title='commands = ["cmd1", "cmd2"]'
          description="Each entry becomes one image layer, applied top to bottom."
          quadlet="RUN <command>"
        />
      </div>
      <button
        type="button"
        onClick={() => setRunCommands([...runCommands, ''])}
        className="text-xs text-[var(--accent-mauve)] hover:text-white flex items-center gap-1 cursor-pointer"
      >
        <Plus className="w-3 h-3" />
        <span>Add</span>
      </button>
    </div>

    {/* Always show at least one row: a blank starter when the list is
        empty. Blank rows never reach the TOML, so this is purely an
        invitation to type — the section reads as missing otherwise. */}
    <div className="space-y-1.5">
      {(runCommands.length > 0 ? runCommands : ['']).map((cmd, idx) => {
        const isStarter = runCommands.length === 0;
        const commit = (val: string) => {
          if (isStarter) {
            setRunCommands([val]);
            return;
          }
          const next = [...runCommands];
          next[idx] = val;
          setRunCommands(next);
        };
        return (
          <div key={idx} className="flex items-center gap-2">
            <span className="w-3 shrink-0 text-right text-[11px] font-mono text-[var(--text-muted)]/50 select-none">
              {idx + 1}
            </span>
            <input
              type="text"
              value={cmd}
              aria-label={`RUN command ${idx + 1}`}
              spellCheck={false}
              placeholder={isStarter ? 'Type a RUN command to add it...' : 'e.g. npm install -g pnpm'}
              onChange={(e) => commit(e.target.value)}
              onKeyDown={(e) => {
                // Enter adds the next layer instead of submitting the form.
                if (e.key === 'Enter') {
                  e.preventDefault();
                  if (isStarter) {
                    const v = e.currentTarget.value;
                    setRunCommands(v.trim() ? [v, ''] : ['']);
                  } else {
                    setRunCommands([...runCommands.slice(0, idx + 1), '', ...runCommands.slice(idx + 1)]);
                  }
                }
              }}
              className={`${STUDIO_FIELD} flex-1`}
            />
            {!isStarter && (
              <button
                type="button"
                onClick={() => setRunCommands(runCommands.filter((_, i) => i !== idx))}
                className="p-1.5 text-[var(--text-muted)] hover:text-[var(--accent-red)] rounded-[2px] cursor-pointer shrink-0"
                title={`Remove RUN command ${idx + 1}`}
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
        );
      })}
    </div>

    <p className="text-[11px] font-mono text-[var(--text-muted)]">
      {runCommands.filter((c) => c.trim()).length} RUN layer
      {runCommands.filter((c) => c.trim()).length === 1 ? '' : 's'}, applied top to bottom
    </p>
    {err('image.run.commands') && (
      <p className="text-[11px] font-mono text-[var(--accent-red)]">{err('image.run.commands')}</p>
    )}
  </div>
</div>
  );
}
