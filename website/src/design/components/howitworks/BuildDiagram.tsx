import React from 'react';
interface BuildDiagramProps { selectedNode: string; onSelect: (id: string) => void; isSimulating: boolean; simStep: number; }

export function BuildDiagram({ selectedNode, onSelect, isSimulating, simStep }: BuildDiagramProps) {
  return (
<svg
  viewBox="0 0 850 310"
  className="w-full max-w-[850px] h-auto select-none overflow-visible"
>
  <defs>
    <linearGradient id="ib-grad1" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stopColor="#3b82f6" />
      <stop offset="100%" stopColor="#6366f1" />
    </linearGradient>
    <linearGradient id="ib-grad2" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stopColor="#8b5cf6" />
      <stop offset="100%" stopColor="#d946ef" />
    </linearGradient>
    <linearGradient id="ib-grad3" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stopColor="#6366f1" />
      <stop offset="100%" stopColor="#a855f7" />
    </linearGradient>
    <marker
      id="ib-arrow"
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

  {/* Connecting Lines */}
  <path
    d="M 160 135 L 210 135"
    stroke="var(--border)"
    strokeWidth="2.5"
    markerEnd="url(#ib-arrow)"
  />
  <path
    d="M 400 135 C 420 135, 420 55, 440 55"
    stroke="var(--border)"
    strokeWidth="2.5"
    markerEnd="url(#ib-arrow)"
  />
  <path
    d="M 400 135 C 420 135, 420 220, 440 220"
    stroke="var(--border)"
    strokeWidth="2.5"
    markerEnd="url(#ib-arrow)"
  />
  <path
    d="M 600 55 C 630 55, 630 135, 660 135"
    stroke="var(--border)"
    strokeWidth="2.5"
    markerEnd="url(#ib-arrow)"
  />
  <path
    d="M 630 220 C 650 220, 650 135, 660 135"
    stroke="var(--border)"
    strokeWidth="2.5"
    markerEnd="url(#ib-arrow)"
  />

  {/* Animated pulse packet when simulating */}
  {isSimulating && (
    <circle r="5" fill="#cba6f7">
      <animateMotion
        path="M 160 135 L 210 135 M 400 135 C 420 135, 420 55, 440 55 M 600 55 C 630 55, 630 135, 660 135"
        dur="3s"
        repeatCount="indefinite"
      />
    </circle>
  )}

  {/* Node 1: definition.toml */}
  <g
    transform="translate(20, 95)"
    onClick={() => onSelect('build_toml')}
    className="cursor-pointer group"
  >
    <rect
      width="140"
      height="80"
      rx="6"
      fill="var(--bg-mantle)"
      stroke={selectedNode === 'build_toml' ? 'var(--accent-mauve)' : 'var(--border)'}
      strokeWidth={selectedNode === 'build_toml' ? '2.5' : '1.5'}
      className="transition-all group-hover:stroke-[var(--accent-mauve)]"
    />
    <text
      x="70"
      y="38"
      textAnchor="middle"
      fill="var(--text-primary)"
      fontSize="13"
      fontWeight="700"
      fontFamily="monospace"
    >
      definition.toml
    </text>
    <text
      x="70"
      y="58"
      textAnchor="middle"
      fill="var(--text-muted)"
      fontSize="11"
      fontFamily="monospace"
    >
      Declarative config
    </text>
  </g>

  {/* Node 2: podbox build / enable */}
  <g
    transform="translate(210, 95)"
    onClick={() => onSelect('build_codegen')}
    className="cursor-pointer group"
  >
    <rect
      width="190"
      height="80"
      rx="6"
      fill="url(#ib-grad1)"
      stroke={selectedNode === 'build_codegen' ? '#ffffff' : '#334155'}
      strokeWidth={selectedNode === 'build_codegen' ? '2.5' : '1'}
      className="transition-all group-hover:brightness-110"
    />
    <text
      x="95"
      y="38"
      textAnchor="middle"
      fill="#ffffff"
      fontSize="13"
      fontWeight="700"
      fontFamily="monospace"
    >
      podbox build
    </text>
    <text
      x="95"
      y="58"
      textAnchor="middle"
      fill="#e2e8f0"
      fontSize="11"
      fontFamily="monospace"
    >
      Pure codegen, no daemon
    </text>
  </g>

  {/* Node 3a: OCI Image */}
  <g
    transform="translate(440, 15)"
    onClick={() => onSelect('build_image')}
    className="cursor-pointer group"
  >
    <rect
      width="160"
      height="80"
      rx="6"
      fill="url(#ib-grad2)"
      stroke={selectedNode === 'build_image' ? '#ffffff' : '#334155'}
      strokeWidth={selectedNode === 'build_image' ? '2.5' : '1'}
      className="transition-all group-hover:brightness-110"
    />
    <text
      x="80"
      y="38"
      textAnchor="middle"
      fill="#ffffff"
      fontSize="13"
      fontWeight="700"
      fontFamily="monospace"
    >
      OCI Image
    </text>
    <text
      x="80"
      y="58"
      textAnchor="middle"
      fill="#e2e8f0"
      fontSize="11"
      fontFamily="monospace"
    >
      Packages baked in
    </text>
  </g>

  {/* Node 3b: Quadlet units */}
  <g
    transform="translate(440, 175)"
    onClick={() => onSelect('build_quadlet')}
    className="cursor-pointer group"
  >
    <rect
      width="190"
      height="90"
      rx="6"
      fill="url(#ib-grad3)"
      stroke={selectedNode === 'build_quadlet' ? '#ffffff' : '#334155'}
      strokeWidth={selectedNode === 'build_quadlet' ? '2.5' : '1'}
      className="transition-all group-hover:brightness-110"
    />
    <text
      x="95"
      y="34"
      textAnchor="middle"
      fill="#ffffff"
      fontSize="13"
      fontWeight="700"
      fontFamily="monospace"
    >
      Quadlet units
    </text>
    <text
      x="95"
      y="54"
      textAnchor="middle"
      fill="#e2e8f0"
      fontSize="11"
      fontFamily="monospace"
    >
      .container .socket .build
    </text>
    <text
      x="95"
      y="70"
      textAnchor="middle"
      fill="#cbd5e1"
      fontSize="10"
      fontFamily="monospace"
    >
      + companion services
    </text>
  </g>

  {/* Node 4: systemd --user */}
  <g
    transform="translate(660, 95)"
    onClick={() => onSelect('build_systemd')}
    className="cursor-pointer group"
  >
    <rect
      width="150"
      height="80"
      rx="6"
      fill="var(--bg-mantle)"
      stroke={selectedNode === 'build_systemd' ? 'var(--accent-blue)' : 'var(--border)'}
      strokeWidth={selectedNode === 'build_systemd' ? '2.5' : '1.5'}
      className="transition-all group-hover:stroke-[var(--accent-blue)]"
    />
    <text
      x="75"
      y="38"
      textAnchor="middle"
      fill="var(--text-primary)"
      fontSize="13"
      fontWeight="700"
      fontFamily="monospace"
    >
      systemd --user
    </text>
    <text
      x="75"
      y="58"
      textAnchor="middle"
      fill="var(--text-muted)"
      fontSize="11"
      fontFamily="monospace"
    >
      Owns the lifecycle
    </text>
  </g>
</svg>
  );
}
