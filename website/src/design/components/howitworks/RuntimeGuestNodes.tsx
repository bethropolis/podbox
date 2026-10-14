import React from 'react';
interface RuntimeGuestNodesProps { selectedNode: string; onSelect: (id: string) => void; }

export function RuntimeGuestNodes({ selectedNode, onSelect }: RuntimeGuestNodesProps) {
  return (
    <>

{/* Top: Guest Daemon in Container */}
<g
  transform="translate(265, 20)"
  onClick={() => onSelect('rt_guest')}
  className="cursor-pointer group"
>
  <rect
    width="320"
    height="78"
    rx="6"
    fill="url(#rt-grad1)"
    stroke={selectedNode === 'rt_guest' ? 'var(--accent-mauve)' : 'color-mix(in srgb, var(--accent-mauve) 35%, var(--border))'}
    strokeWidth={selectedNode === 'rt_guest' ? '2.5' : '1'}
    className="transition-all group-hover:brightness-110"
  />
  <text
    x="160"
    y="34"
    textAnchor="middle"
    fill="var(--text-primary)"
    fontSize="13"
    fontWeight="700"
    fontFamily="JetBrains Mono, monospace"
  >
    podbox-guest (in container)
  </text>
  <text
    x="160"
    y="56"
    textAnchor="middle"
    fill="var(--text-subtext)"
    fontSize="11"
    fontFamily="JetBrains Mono, monospace"
  >
    notify · clipboard · xdg-open · host-exec
  </text>
</g>

{/* Handshake line */}
<path
  d="M 425 98 L 425 198"
  stroke="var(--accent-mauve)"
  strokeWidth="2.5"
  markerEnd="url(#rt-arrow)"
  className="cursor-pointer"
  onClick={() => onSelect('rt_handshake')}
/>
<text
  x="440"
  y="148"
  fill="var(--accent-mauve)"
  fontSize="10"
  fontFamily="JetBrains Mono, monospace"
  fontWeight="600"
>
  Hello ↔ capability negotiation
</text>

{/* Dashed proxy connections */}
<path
    fill="none"
  d="M 360 98 C 360 150, 220 150, 220 198"
  stroke="var(--border)"
  strokeWidth="2"
  strokeDasharray="4,4"
  markerEnd="url(#rt-arrow)"
/>
<path
    fill="none"
  d="M 490 98 C 490 150, 630 150, 630 198"
  stroke="var(--border)"
  strokeWidth="2"
  strokeDasharray="4,4"
  markerEnd="url(#rt-arrow)"
/>

    </>
  );
}
