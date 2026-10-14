import React from 'react';
import { AlertTriangle, Info, Lightbulb, ShieldAlert, CircleAlert } from 'lucide-react';

interface AdmonitionProps {
  type: 'note' | 'tip' | 'warning' | 'info' | 'danger';
  title?: string;
  children: React.ReactNode;
}

// One accent per kind, used only for the rule, icon and title; the body stays
// in the normal text colour so long callouts remain easy to read.
const KINDS = {
  note: { accent: 'var(--accent-blue)', Icon: Info, label: 'Note' },
  info: { accent: 'var(--accent-mauve)', Icon: CircleAlert, label: 'Info' },
  tip: { accent: 'var(--accent-green)', Icon: Lightbulb, label: 'Tip' },
  warning: { accent: 'var(--accent-peach)', Icon: AlertTriangle, label: 'Warning' },
  danger: { accent: 'var(--accent-red)', Icon: ShieldAlert, label: 'Danger' },
} as const;

export function Admonition({ type = 'note', title, children }: AdmonitionProps) {
  const kind = KINDS[type] ?? KINDS.note;
  const { Icon } = kind;
  const heading = title && title.trim() ? title : kind.label;

  return (
    <aside
      role="note"
      className="my-6 rounded-md border border-[var(--border)] border-l-[3px] px-4 py-3.5 text-[15px] leading-7"
      style={{
        borderLeftColor: kind.accent,
        background: `color-mix(in srgb, ${kind.accent} 6%, transparent)`,
      }}
    >
      <div className="mb-1 flex items-center gap-2 text-sm font-semibold" style={{ color: kind.accent }}>
        <Icon className="w-4 h-4 shrink-0" aria-hidden="true" />
        <span>{heading}</span>
      </div>
      <div className="text-[var(--text-primary)] [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&_p]:text-[var(--text-primary)]">
        {children}
      </div>
    </aside>
  );
}
