import React from 'react';
import { withBase } from '../../base';
import { ArrowRight } from 'lucide-react';
import type { NodeDetail } from './nodeData';

export function NodeInspector({ node }: { node: NodeDetail }) {
  return (
    <div className="grid gap-6 p-5 sm:p-6 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
      <div>
        <div className="font-mono text-[11px] text-[var(--text-muted)]">{node.category}</div>
        <h3 className="mt-1 text-lg font-semibold tracking-tight text-[var(--text-primary)]">
          {node.title}
        </h3>
        <p className="mt-2 text-sm leading-relaxed text-[var(--text-subtext)]">{node.description}</p>
        <a
          href={withBase('/docs/architecture/')}
          className="mt-4 inline-flex items-center gap-1.5 text-sm text-[var(--accent-blue)] hover:underline underline-offset-4"
        >
          Architecture docs
          <ArrowRight className="w-3.5 h-3.5" />
        </a>
      </div>

      <ul className="self-start divide-y divide-[var(--border)] border-y border-[var(--border)] font-mono text-xs text-[var(--text-subtext)]">
        {node.specs.map((spec) => (
          <li key={spec} className="py-2.5 leading-relaxed">
            {spec}
          </li>
        ))}
      </ul>
    </div>
  );
}
