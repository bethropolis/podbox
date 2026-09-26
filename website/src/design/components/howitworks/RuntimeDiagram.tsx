import React from 'react';
import { RuntimeGuestNodes } from './RuntimeGuestNodes';
import { RuntimeHostNodes } from './RuntimeHostNodes';

interface RuntimeDiagramProps { selectedNode: string; onSelect: (id: string) => void; activePacket: string | null; }

export function RuntimeDiagram({ selectedNode, onSelect, activePacket }: RuntimeDiagramProps) {
  return (
<svg
  viewBox="0 0 850 440"
  className="w-full max-w-[850px] h-auto select-none overflow-visible"
>
  <defs>
    <linearGradient id="rt-grad1" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stopColor="#3b82f6" />
      <stop offset="100%" stopColor="#6366f1" />
    </linearGradient>
    <linearGradient id="rt-grad3" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stopColor="#6366f1" />
      <stop offset="100%" stopColor="#a855f7" />
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
      <RuntimeGuestNodes selectedNode={selectedNode} onSelect={onSelect} activePacket={activePacket} />
      <RuntimeHostNodes selectedNode={selectedNode} onSelect={onSelect} />
    </svg>
  );
}
