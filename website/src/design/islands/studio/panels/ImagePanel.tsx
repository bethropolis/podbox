import React from 'react';
import {
  Box,
} from 'lucide-react';
import {
  StudioInput,
  StudioSelect,
  StudioTagInput,
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
      Define the upstream base OS or OCI image, baked packages, and Containerfile RUN steps.
    </p>
  </div>

  {/* Base Image Selection Type */}
  <div className="space-y-2">
    <div className="flex items-center">
      <span className="text-xs font-medium text-[var(--text-subtext)]">Base OS Source</span>
      <StudioTooltip
        section="[image]"
        title="preset vs base"
        description="Choose from curated tested distributions (Fedora, Ubuntu, Arch, Alpine, Debian) or specify any OCI image URI from ghcr.io or docker.io."
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
            description="podbox automatically handles package manager configuration, baked-in guest interceptors, and user IDs for verified distributions."
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
          description="Packages to bake directly into the OCI image at build time using the distribution's native package manager."
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
          description="Unwanted stock packages purged during image synthesis to reduce size."
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
            description="Force package manager binary (dnf, apt, pacman, apk) or let podbox detect automatically from base OS."
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
            description="Number of times podman will retry pulling layers over flaky networks."
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

  {/* Extra Run commands */}
  <div className="space-y-1.5">
    <div className="flex items-center">
      <span className="text-xs font-medium text-[var(--text-subtext)]">Extra Containerfile RUN Commands</span>
      <StudioTooltip
        section="[image]"
        title="run = [&quot;cmd1&quot;, &quot;cmd2&quot;]"
        description="Custom shell commands executed inside the build container to configure dotfiles, compilers, or custom software."
        quadlet="RUN <command>"
      />
    </div>
    <textarea
      value={runCommands}
      onChange={(e) => setRunCommands(e.target.value)}
      rows={2}
      placeholder="e.g. dnf clean all"
      className="w-full px-3 py-2 text-xs font-mono rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent-mauve)] focus:ring-1 focus:ring-[var(--accent-mauve)]/30 transition-all resize-y"
    />
  </div>
</div>
  );
}
