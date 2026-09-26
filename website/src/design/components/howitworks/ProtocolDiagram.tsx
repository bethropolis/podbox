import React from 'react';
interface ProtocolDiagramProps { selectedNode: string; onSelect: (id: string) => void; }

export function ProtocolDiagram({ selectedNode, onSelect }: ProtocolDiagramProps) {
  return (
<svg
  viewBox="0 0 850 320"
  className="w-full max-w-[850px] h-auto select-none overflow-visible"
>
  <defs>
    <linearGradient id="pr-grad" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stopColor="#fab387" />
      <stop offset="100%" stopColor="#f38ba8" />
    </linearGradient>
  </defs>

  {/* Client Box */}
  <g
    transform="translate(40, 60)"
    onClick={() => onSelect('proto_handshake')}
    className="cursor-pointer group"
  >
    <rect
      width="220"
      height="200"
      rx="6"
      fill="var(--bg-mantle)"
      stroke="var(--accent-mauve)"
      strokeWidth="2"
    />
    <text
      x="110"
      y="35"
      textAnchor="middle"
      fill="var(--accent-mauve)"
      fontSize="14"
      fontWeight="700"
      fontFamily="monospace"
    >
      GUEST CLIENT
    </text>
    <text
      x="110"
      y="60"
      textAnchor="middle"
      fill="var(--text-muted)"
      fontSize="11"
      fontFamily="monospace"
    >
      /usr/local/bin/podbox-guest
    </text>

    <rect
      x="20"
      y="85"
      width="180"
      height="35"
      rx="3"
      fill="var(--bg-base)"
      stroke="var(--border)"
    />
    <text
      x="110"
      y="107"
      textAnchor="middle"
      fill="var(--text-primary)"
      fontSize="11"
      fontFamily="monospace"
    >
      1. Client Hello (JSON)
    </text>

    <rect
      x="20"
      y="135"
      width="180"
      height="35"
      rx="3"
      fill="var(--bg-base)"
      stroke="var(--border)"
    />
    <text
      x="110"
      y="157"
      textAnchor="middle"
      fill="var(--text-primary)"
      fontSize="11"
      fontFamily="monospace"
    >
      3. Stream Request Frames
    </text>
  </g>

  {/* Host Box */}
  <g
    transform="translate(590, 60)"
    onClick={() => onSelect('proto_channels')}
    className="cursor-pointer group"
  >
    <rect
      width="220"
      height="200"
      rx="6"
      fill="var(--bg-mantle)"
      stroke="var(--accent-blue)"
      strokeWidth="2"
    />
    <text
      x="110"
      y="35"
      textAnchor="middle"
      fill="var(--accent-blue)"
      fontSize="14"
      fontWeight="700"
      fontFamily="monospace"
    >
      HOST BROKER
    </text>
    <text
      x="110"
      y="60"
      textAnchor="middle"
      fill="var(--text-muted)"
      fontSize="11"
      fontFamily="monospace"
    >
      /run/user/1000/podbox.sock
    </text>

    <rect
      x="20"
      y="85"
      width="180"
      height="35"
      rx="3"
      fill="var(--bg-base)"
      stroke="var(--border)"
    />
    <text
      x="110"
      y="107"
      textAnchor="middle"
      fill="var(--text-primary)"
      fontSize="11"
      fontFamily="monospace"
    >
      2. Verify &amp; HelloAck
    </text>

    <rect
      x="20"
      y="135"
      width="180"
      height="35"
      rx="3"
      fill="var(--bg-base)"
      stroke="var(--border)"
    />
    <text
      x="110"
      y="157"
      textAnchor="middle"
      fill="var(--text-primary)"
      fontSize="11"
      fontFamily="monospace"
    >
      4. Dispatch to Session
    </text>
  </g>

  {/* Middle Protocol channel */}
  <g transform="translate(280, 100)">
    <path
      d="M 0 50 L 290 50"
      stroke="var(--accent-green)"
      strokeWidth="3"
      strokeDasharray="6,4"
    />
    <rect
      x="55"
      y="30"
      width="180"
      height="40"
      rx="4"
      fill="var(--bg-crust)"
      stroke="var(--border)"
    />
    <text
      x="145"
      y="55"
      textAnchor="middle"
      fill="var(--accent-green)"
      fontSize="11"
      fontWeight="700"
      fontFamily="monospace"
    >
      UNIX DOMAIN SOCKET
    </text>
  </g>
</svg>
  );
}
