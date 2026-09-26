import React from 'react';
import { withBase } from '../../base';
import {
  CheckCircle2,
  ChevronRight,
} from 'lucide-react';
import type { NodeDetail } from './nodeData';

export function NodeInspector({ node }: { node: NodeDetail }) {
  return (
<div className="p-4 sm:p-5 space-y-4 max-w-3xl">
  <div className="flex items-center justify-between">
    <span className="text-[10px] uppercase font-bold px-2 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-mauve)] border border-[var(--border)]">
      {node.category}
    </span>
    <span className="text-xs text-[var(--text-muted)] flex items-center gap-1">
      <CheckCircle2 className="w-3.5 h-3.5 text-[var(--accent-green)]" />
      <span>Verified Spec</span>
    </span>
  </div>

  <div>
    <h3 className="text-base font-bold text-[var(--text-primary)]">
      {node.title}
    </h3>
    <p className="mt-1.5 text-xs text-[var(--text-subtext)] leading-relaxed">
      {node.description}
    </p>
  </div>

  <div className="space-y-1.5 pt-1">
    <div className="text-[11px] font-bold text-[var(--text-muted)] uppercase tracking-wider">
      Key Architecture Guarantees
    </div>
    <ul className="space-y-1 text-xs text-[var(--text-subtext)]">
      {node.specs.map((spec, sIdx) => (
        <li key={sIdx} className="flex items-start gap-2">
          <span className="text-[var(--accent-mauve)] font-bold">›</span>
          <span>{spec}</span>
        </li>
      ))}
    </ul>
  </div>

  {/* Quick CLI trigger or doc jump */}
  <div className="pt-2">
    <a
      href={withBase('/docs/architecture')}
      className="text-xs text-[var(--accent-blue)] hover:underline inline-flex items-center gap-1 cursor-pointer font-medium"
    >
      <span>Explore full architecture documentation</span>
      <ChevronRight className="w-3.5 h-3.5" />
    </a>
  </div>
</div>
  );
}
