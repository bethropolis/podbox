import React from 'react';
interface RuntimeHostNodesProps { selectedNode: string; onSelect: (id: string) => void; }

export function RuntimeHostNodes({ selectedNode, onSelect }: RuntimeHostNodesProps) {
  return (
    <>
{/* HOST SECURITY BOUNDARY line */}
<line
  x1="30"
  y1="150"
  x2="820"
  y2="150"
  stroke="var(--border)"
  strokeWidth="1.5"
  strokeDasharray="6,6"
/>
<text
  x="40"
  y="142"
  fill="var(--text-muted)"
  fontSize="10"
  fontWeight="700"
  fontFamily="monospace"
  letterSpacing="1"
>
  HOST SECURITY BOUNDARY (RESTRICTED ACCESS)
</text>

{/* Wayland Proxy */}
<g
  transform="translate(130, 200)"
  onClick={() => onSelect('rt_wayland')}
  className="cursor-pointer group"
>
  <rect
    width="180"
    height="80"
    rx="6"
    fill="url(#rt-grad3)"
    stroke={selectedNode === 'rt_wayland' ? '#ffffff' : '#334155'}
    strokeWidth={selectedNode === 'rt_wayland' ? '2.5' : '1'}
    className="transition-all group-hover:brightness-110"
  />
  <text
    x="90"
    y="34"
    textAnchor="middle"
    fill="#ffffff"
    fontSize="13"
    fontWeight="700"
    fontFamily="monospace"
  >
    Wayland proxy
  </text>
  <text
    x="90"
    y="54"
    textAnchor="middle"
    fill="#e2e8f0"
    fontSize="11"
    fontFamily="monospace"
  >
    Filters registry globals
  </text>
</g>

{/* Socket Host */}
<g
  transform="translate(335, 200)"
  onClick={() => onSelect('rt_socket')}
  className="cursor-pointer group"
>
  <rect
    width="180"
    height="80"
    rx="6"
    fill="url(#rt-grad3)"
    stroke={selectedNode === 'rt_socket' ? '#ffffff' : '#334155'}
    strokeWidth={selectedNode === 'rt_socket' ? '2.5' : '1'}
    className="transition-all group-hover:brightness-110"
  />
  <text
    x="90"
    y="34"
    textAnchor="middle"
    fill="#ffffff"
    fontSize="13"
    fontWeight="700"
    fontFamily="monospace"
  >
    Socket host
  </text>
  <text
    x="90"
    y="54"
    textAnchor="middle"
    fill="#e2e8f0"
    fontSize="11"
    fontFamily="monospace"
  >
    Capability-gated handshake
  </text>
</g>

{/* D-Bus Proxy */}
<g
  transform="translate(540, 200)"
  onClick={() => onSelect('rt_dbus')}
  className="cursor-pointer group"
>
  <rect
    width="180"
    height="80"
    rx="6"
    fill="url(#rt-grad3)"
    stroke={selectedNode === 'rt_dbus' ? '#ffffff' : '#334155'}
    strokeWidth={selectedNode === 'rt_dbus' ? '2.5' : '1'}
    className="transition-all group-hover:brightness-110"
  />
  <text
    x="90"
    y="34"
    textAnchor="middle"
    fill="#ffffff"
    fontSize="13"
    fontWeight="700"
    fontFamily="monospace"
  >
    D-Bus proxy
  </text>
  <text
    x="90"
    y="54"
    textAnchor="middle"
    fill="#e2e8f0"
    fontSize="11"
    fontFamily="monospace"
  >
    Scoped portal call rules
  </text>
</g>

{/* Fan-in to Host resources */}
<path
  d="M 220 280 C 220 310, 340 310, 340 328"
  stroke="var(--border)"
  strokeWidth="2"
  strokeDasharray="4,4"
  markerEnd="url(#rt-arrow)"
/>
<path
  d="M 425 280 L 425 328"
  stroke="var(--border)"
  strokeWidth="2"
  strokeDasharray="4,4"
  markerEnd="url(#rt-arrow)"
/>
<path
  d="M 630 280 C 630 310, 510 310, 510 328"
  stroke="var(--border)"
  strokeWidth="2"
  strokeDasharray="4,4"
  markerEnd="url(#rt-arrow)"
/>

{/* Host Resources Bottom Box */}
<g
  transform="translate(265, 330)"
  onClick={() => onSelect('rt_host')}
  className="cursor-pointer group"
>
  <rect
    width="320"
    height="80"
    rx="6"
    fill="var(--bg-mantle)"
    stroke={selectedNode === 'rt_host' ? 'var(--accent-teal)' : 'var(--border)'}
    strokeWidth={selectedNode === 'rt_host' ? '2.5' : '1.5'}
    className="transition-all group-hover:stroke-[var(--accent-teal)]"
  />
  <text
    x="160"
    y="34"
    textAnchor="middle"
    fill="var(--text-primary)"
    fontSize="13"
    fontWeight="700"
    fontFamily="monospace"
  >
    Host resources
  </text>
  <text
    x="160"
    y="54"
    textAnchor="middle"
    fill="var(--text-muted)"
    fontSize="11"
    fontFamily="monospace"
  >
    Compositor · session bus · GPU · PipeWire · XDG
  </text>
</g>
    </>
  );
}
