import React, { useId } from 'react';

interface PodboxLogoProps {
  className?: string;
  size?: number;
}

export function PodboxLogo({ className = 'w-6 h-6', size }: PodboxLogoProps) {
  const id = useId().replace(/:/g, '');
  const gradId = `podbox-g-${id}`;
  const maskId = `podbox-split-${id}`;

  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 512 512"
      width={size}
      height={size}
      className={className}
      fill="none"
      aria-label="podbox logo"
    >
      <defs>
        <linearGradient id={gradId} x1="0%" y1="0%" x2="100%" y2="100%" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#3b82f6" />
          <stop offset="25%" stopColor="#6366f1" />
          <stop offset="50%" stopColor="#8b5cf6" />
          <stop offset="75%" stopColor="#d946ef" />
          <stop offset="100%" stopColor="#ec4899" />
        </linearGradient>
        <mask id={maskId}>
          <rect x="0" y="0" width="512" height="512" fill="#ffffff" />
          <rect x="0" y="244" width="512" height="24" fill="#000000" />
        </mask>
      </defs>
      <rect
        x="72"
        y="72"
        width="368"
        height="368"
        rx="88"
        fill="none"
        stroke={`url(#${gradId})`}
        strokeWidth="44"
      />
      <rect
        x="206"
        y="156"
        width="100"
        height="200"
        rx="50"
        fill={`url(#${gradId})`}
        mask={`url(#${maskId})`}
      />
    </svg>
  );
}
