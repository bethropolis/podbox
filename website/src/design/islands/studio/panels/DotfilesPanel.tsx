import React from 'react';
import {
  FolderGit2,
} from 'lucide-react';
import {
  StudioInput,
  StudioSelect,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import { DOTFILES_CLONE_OPTIONS } from '../schema';
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
      Populate the container home with your dotfiles from a directory on the host or a git URL.
      Empty source skips this entirely.
    </p>
  </div>

  <StudioInput
    label={
      <div className="flex items-center">
        <span>Source</span>
        <StudioTooltip
          section="[dotfiles]"
          title='source = "host:~/.dotfiles"'
          description="Where the files come from. A host: directory is copied in; an https:// git URL is cloned. Either way it's a copy, not a live mount — host edits arrive on the next dotfiles sync."
          quadlet="Volume=%h/containers/dev/.dotfiles:/home/user/.dotfiles"
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
            title='target = "~/.dotfiles"'
            description="Where the files land inside the container home. Must stay inside it. Defaults to ~/.dotfiles."
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
            title='clone_on = "host"'
            description="Where a git source gets cloned. Host reuses your SSH agent and credentials (private repos just work); container keeps the host untouched but needs its own credentials. Ignored for host: sources."
          />
        </div>
      }
      value={dotfilesCloneOn}
      onChange={setDotfilesCloneOn}
      options={DOTFILES_CLONE_OPTIONS}
      error={err('dotfiles.clone_on')}
    />
  </div>

  <StudioInput
    label={
      <div className="flex items-center">
        <span>Install Command</span>
        <StudioTooltip
          section="[dotfiles]"
          title='install = "./install.sh"'
          description="Runs once inside the container after the files land, from the target directory — e.g. stow, chezmoi apply, or ./install.sh. Leave empty if the files work as-is."
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
