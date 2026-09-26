import React from 'react';
import {
  Bell,
  Clipboard,
  Globe,
  Sparkles,
  Terminal,
} from 'lucide-react';
import type { RuntimeAction } from './useSimulation';

export function RuntimeTriggerBar({ onTrigger }: { onTrigger: (a: RuntimeAction) => void }) {
  return (
<div className="flex flex-wrap items-center justify-between px-4 py-2 bg-[var(--bg-base)] border-b border-[var(--border)] text-xs gap-2">
  <div className="flex items-center gap-2 text-[var(--accent-mauve)] font-medium">
    <Sparkles className="w-3.5 h-3.5" />
    <span>Trigger In-Container Event:</span>
  </div>
  <div className="flex flex-wrap items-center gap-1.5">
    <button
      onClick={() => onTrigger('notification')}
      type="button"
      className="flex items-center gap-1 px-2.5 py-1 rounded-[2px] bg-[var(--bg-mantle)] hover:bg-[var(--bg-surface0)] border border-[var(--border)] text-[var(--accent-yellow)] cursor-pointer"
    >
      <Bell className="w-3 h-3" />
      <span>notify-send</span>
    </button>
    <button
      onClick={() => onTrigger('clipboard')}
      type="button"
      className="flex items-center gap-1 px-2.5 py-1 rounded-[2px] bg-[var(--bg-mantle)] hover:bg-[var(--bg-surface0)] border border-[var(--border)] text-[var(--accent-green)] cursor-pointer"
    >
      <Clipboard className="w-3 h-3" />
      <span>wl-copy</span>
    </button>
    <button
      onClick={() => onTrigger('url')}
      type="button"
      className="flex items-center gap-1 px-2.5 py-1 rounded-[2px] bg-[var(--bg-mantle)] hover:bg-[var(--bg-surface0)] border border-[var(--border)] text-[var(--accent-blue)] cursor-pointer"
    >
      <Globe className="w-3 h-3" />
      <span>xdg-open</span>
    </button>
    <button
      onClick={() => onTrigger('exec')}
      type="button"
      className="flex items-center gap-1 px-2.5 py-1 rounded-[2px] bg-[var(--bg-mantle)] hover:bg-[var(--bg-surface0)] border border-[var(--border)] text-[var(--accent-peach)] cursor-pointer"
    >
      <Terminal className="w-3 h-3" />
      <span>host-exec</span>
    </button>
  </div>
</div>
  );
}
