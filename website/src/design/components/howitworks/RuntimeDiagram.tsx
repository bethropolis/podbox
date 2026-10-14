import React from 'react';
import { RuntimeGuestNodes } from './RuntimeGuestNodes';
import { RuntimeHostNodes } from './RuntimeHostNodes';

interface RuntimeDiagramProps { selectedNode: string; onSelect: (id: string) => void; }

export function RuntimeDiagram({ selectedNode, onSelect }: RuntimeDiagramProps) {
  return (
<svg
  viewBox="0 0 850 440"
  className="w-full max-w-[850px] h-auto select-none overflow-visible"
>
  <defs>
    <linearGradient id="rt-grad1" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stopColor="color-mix(in srgb, var(--accent-blue) 16%, var(--bg-mantle))" />
      <stop offset="100%" stopColor="color-mix(in srgb, var(--accent-blue) 16%, var(--bg-mantle))" />
    </linearGradient>
    <linearGradient id="rt-grad3" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stopColor="color-mix(in srgb, var(--accent-blue) 16%, var(--bg-mantle))" />
      <stop offset="100%" stopColor="color-mix(in srgb, var(--accent-mauve) 16%, var(--bg-mantle))" />
    </linearGradient>
    <marker
      id="rt-arrow"
      viewBox="0 0 10 10"
      refX="6"
      refY="5"
      markerWidth="6"
      markerHeight="6"
      orient="auto-start-reverse"
    >
      <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="var(--text-muted)" />
    </marker>
  </defs>
      <RuntimeGuestNodes selectedNode={selectedNode} onSelect={onSelect} />
      <RuntimeHostNodes selectedNode={selectedNode} onSelect={onSelect} />
    </svg>
  );
}
