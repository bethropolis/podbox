import React from 'react';
import {
  FolderGit2,
} from 'lucide-react';
import {
  StudioInput,
  StudioSelect,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import type { StudioState } from '../useStudioState';

type DotfilesPanelProps = Pick<StudioState, 'dotfilesCloneOn' | 'dotfilesInstall' | 'dotfilesSource' | 'dotfilesTarget' | 'setDotfilesCloneOn' | 'setDotfilesInstall' | 'setDotfilesSource' | 'setDotfilesTarget'>;

export function DotfilesPanel({ st, errorMap }: { st: DotfilesPanelProps; errorMap?: Record<string, string> }) {
  const { dotfilesCloneOn, dotfilesInstall, dotfilesSource, dotfilesTarget, setDotfilesCloneOn, setDotfilesInstall, setDotfilesSource, setDotfilesTarget } = st;
  const err = (field: string) => errorMap?.[field];
  return (
<div className="space-y-5 animate-fadeIn">
  <div className="pb-3 border-b border-[var(--border)]">
    <div className="flex items-center gap-2">
      <FolderGit2 className="w-4 h-4 text-[var(--accent-mauve)]" />
      <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
        [dotfiles] — Repo &amp; Install
      </h2>
    </div>
    <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
      Provision the container home from a dotfiles repo. Empty source disables provisioning entirely.
    </p>
  </div>

  <StudioInput
    label={
      <div className="flex items-center">
        <span>Source</span>
        <StudioTooltip
          section="[dotfiles]"
          title="source = &quot;host:~/.dotfiles&quot;"
          description="Dotfiles origin: host:~/path for a host directory, or an https git URL cloned on first start."
        />
      </div>
    }
    value={dotfilesSource}
    onChange={(e) => setDotfilesSource(e.target.value)}
    placeholder="host:~/.dotfiles or https://github.com/user/dotfiles"
    id="studio-input-dotfiles-source"
    error={err('dotfiles.source')}
  />

  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
    <StudioInput
      label={
        <div className="flex items-center">
          <span>Target</span>
          <StudioTooltip
            section="[dotfiles]"
            title="target = &quot;~/.dotfiles&quot;"
            description="Checkout location inside the container home. Defaults to ~/.dotfiles when empty."
          />
        </div>
      }
      value={dotfilesTarget}
      onChange={(e) => setDotfilesTarget(e.target.value)}
      placeholder="~/.dotfiles"
      id="studio-input-dotfiles-target"
      error={err('dotfiles.target')}
    />

    <StudioSelect
      label={
        <div className="flex items-center">
          <span>Clone Location</span>
          <StudioTooltip
            section="[dotfiles]"
            title="clone_on = &quot;host&quot;"
            description="Clone on the host (shared across rebuilds) or inside the container (isolated per image build)."
          />
        </div>
      }
      value={dotfilesCloneOn}
      onChange={setDotfilesCloneOn}
      options={[
        { value: 'host', label: 'host (shared, cached)' },
        { value: 'container', label: 'container (isolated)' },
      ]}
      error={err('dotfiles.clone_on')}
    />
  </div>

  <StudioInput
    label={
      <div className="flex items-center">
        <span>Install Command</span>
        <StudioTooltip
          section="[dotfiles]"
          title="install = &quot;./install.sh&quot;"
          description="Optional command run inside the container after checkout (e.g. stow, chezmoi apply)."
        />
      </div>
    }
    value={dotfilesInstall}
    onChange={(e) => setDotfilesInstall(e.target.value)}
    placeholder="./install.sh"
    id="studio-input-dotfiles-install"
    error={err('dotfiles.install')}
  />
</div>
  );
}
