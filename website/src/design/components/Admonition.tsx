import React from 'react';
import { AlertTriangle, CheckCircle2, Info, Lightbulb, ShieldAlert } from 'lucide-react';

interface AdmonitionProps {
  type: 'note' | 'tip' | 'warning' | 'info' | 'danger';
  title?: string;
  children: React.ReactNode;
}

export function Admonition({ type = 'note', title, children }: AdmonitionProps) {
  const configs = {
    note: {
      border: 'border-l-[var(--accent-blue)]',
      bg: 'bg-[var(--accent-blue)]/5',
      badge: 'text-[var(--accent-blue)]',
      icon: <Info className="w-4 h-4 text-[var(--accent-blue)] shrink-0" />,
      defaultTitle: 'NOTE',
    },
    tip: {
      border: 'border-l-[var(--accent-green)]',
      bg: 'bg-[var(--accent-green)]/5',
      badge: 'text-[var(--accent-green)]',
      icon: <Lightbulb className="w-4 h-4 text-[var(--accent-green)] shrink-0" />,
      defaultTitle: 'TIP',
    },
    warning: {
      border: 'border-l-[var(--accent-peach)]',
      bg: 'bg-[var(--accent-peach)]/5',
      badge: 'text-[var(--accent-peach)]',
      icon: <AlertTriangle className="w-4 h-4 text-[var(--accent-peach)] shrink-0" />,
      defaultTitle: 'WARNING',
    },
    danger: {
      border: 'border-l-[var(--accent-red)]',
      bg: 'bg-[var(--accent-red)]/5',
      badge: 'text-[var(--accent-red)]',
      icon: <ShieldAlert className="w-4 h-4 text-[var(--accent-red)] shrink-0" />,
      defaultTitle: 'CRITICAL',
    },
    info: {
      border: 'border-l-[var(--accent-mauve)]',
      bg: 'bg-[var(--accent-mauve)]/5',
      badge: 'text-[var(--accent-mauve)]',
      icon: <CheckCircle2 className="w-4 h-4 text-[var(--accent-mauve)] shrink-0" />,
      defaultTitle: 'INFO',
    },
  };

  const config = configs[type] || configs.note;
  const displayTitle = title && title.trim().length > 0 ? title : config.defaultTitle;

  return (
    <div
      className={`my-4 border border-[var(--border)] border-l-4 ${config.border} ${config.bg} rounded-[2px] p-3 text-xs md:text-sm font-mono leading-relaxed`}
    >
      <div className="flex items-center gap-2 mb-1.5 font-bold tracking-wider">
        {config.icon}
        <span className={`uppercase text-xs font-semibold ${config.badge}`}>
          {displayTitle}
        </span>
      </div>
      <div className="text-[var(--text-primary)] pl-6 text-[13px] leading-relaxed">
        {children}
      </div>
    </div>
  );
}
