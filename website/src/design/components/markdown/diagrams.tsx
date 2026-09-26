import React from 'react';
import { withBase } from '../../base';

// architecture.md embeds diagrams as raw <picture> blocks. `marked` emits
// them as paragraph or html tokens depending on context; either way, swap
// them for the framed figure the design intends. Returns null when the text
// holds no known diagram.
const DIAGRAMS: Record<string, string> = {
  'architecture-build': 'podbox build-time architecture',
  'architecture-runtime': 'podbox runtime architecture',
  'codegen_pipeline': 'Codegen pipeline',
  'socket_protocol': 'Host-guest socket protocol',
  'runtime_flow': 'Runtime flow',
  'how_it_works': 'How podbox works',
};

export function diagramSwap(text: string): { src: string; alt: string } | 'logo' | null {
  if (!text.includes('<picture>')) return null;
  if (text.includes('podbox-logo')) return 'logo';
  for (const [name, alt] of Object.entries(DIAGRAMS)) {
    if (text.includes(name)) return { src: withBase(`/assets/${name}.svg`), alt };
  }
  return null;
}

export function LogoFigure() {
  return (
    <div className="my-6 flex justify-center">
      <img src={withBase('/assets/podbox-logo.svg')} alt="podbox logo" className="max-w-[320px] h-auto" />
    </div>
  );
}

export function DiagramFigure({ src, alt }: { src: string; alt: string }) {
  return (
    <div className="my-6 p-4 rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)] flex justify-center">
      <img
        src={src}
        alt={alt}
        className="w-full max-w-[780px] h-auto"
      />
    </div>
  );
}
