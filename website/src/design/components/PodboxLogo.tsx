import React from 'react';

interface PodboxLogoProps {
  className?: string;
  size?: number;
}

// Same geometry as scripts/gen-brand.py (docs/assets/podbox-mark.svg), drawn
// with the live theme tokens so it follows the light/dark toggle.
const TOP = 'M181 245 V197 A75 75 0 0 1 331 197 V245 Z';
const BOTTOM = 'M181 267 H331 V315 A75 75 0 0 1 181 315 Z';

export function PodboxLogo({ className = 'w-6 h-6', size }: PodboxLogoProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 512 512"
      width={size}
      height={size}
      className={className}
      fill="none"
      role="img"
      aria-label="podbox"
    >
      <rect x="56" y="56" width="400" height="400" rx="104" stroke="var(--accent-mauve)" strokeWidth="44" />
      <path d={TOP} fill="var(--accent-mauve)" />
      <path d={BOTTOM} fill="var(--accent-lavender)" />
    </svg>
  );
}
