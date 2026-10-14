import React from 'react';
import { withBase } from '../../base';

// architecture.md and protocol.md embed diagrams as raw <picture> blocks
// (that is what renders on GitHub). Here they are swapped for the same SVG
// inlined into the page, so the diagram picks up the site's theme tokens and
// follows the light/dark toggle instead of the OS setting. The SVGs come from
// scripts/gen-diagrams.py.
const DIAGRAMS: Record<string, string> = {
  'architecture-build': 'podbox at build time',
  'architecture-runtime': 'podbox while a container runs',
  codegen_pipeline: 'Codegen pipeline',
  socket_protocol: 'Host-guest socket protocol',
  runtime_flow: 'Runtime flow, from login to a shell',
  how_it_works: 'How podbox works',
  wire_format: 'Message wire format',
};

// Raw SVG source, bundled at build time. Empty when the glob misses (e.g. a
// diagram not synced yet), in which case the figure falls back to <img>.
const SOURCES = import.meta.glob('/public/assets/*.svg', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

function inlineSvg(name: string): string | null {
  const raw = SOURCES[`/public/assets/${name}.svg`];
  if (!raw) return null;
  // Let CSS size it: drop the fixed width/height, keep the viewBox.
  return raw.replace(/<svg([^>]*?)\swidth="\d+"\sheight="\d+"/, '<svg$1');
}

export function diagramSwap(text: string): { name: string; alt: string } | 'logo' | null {
  if (!text.includes('<picture>')) return null;
  if (text.includes('podbox-logo')) return 'logo';
  for (const [name, alt] of Object.entries(DIAGRAMS)) {
    if (text.includes(`${name}.svg`)) return { name, alt };
  }
  return null;
}

export function LogoFigure() {
  const svg = inlineSvg('podbox-logo');
  return (
    <div className="my-8 flex justify-center">
      {svg ? (
        <div className="w-full max-w-[360px] [&>svg]:block [&>svg]:h-auto [&>svg]:w-full" dangerouslySetInnerHTML={{ __html: svg }} />
      ) : (
        <img src={withBase('/assets/podbox-logo.svg')} alt="podbox" className="max-w-[360px] h-auto" />
      )}
    </div>
  );
}

export function DiagramFigure({ name, alt }: { name: string; alt: string }) {
  const svg = inlineSvg(name);
  return (
    <figure className="my-8 xl:-mx-8">
      {/* Wide diagrams keep a readable size on phones by scrolling sideways. */}
      <div className="overflow-x-auto rounded-md border border-[var(--border)] bg-[var(--bg-base)] p-3 sm:p-5">
        {svg ? (
          <div
            className="min-w-[620px] [&>svg]:block [&>svg]:h-auto [&>svg]:w-full"
            dangerouslySetInnerHTML={{ __html: svg }}
          />
        ) : (
          <img src={withBase(`/assets/${name}.svg`)} alt={alt} className="min-w-[620px] w-full h-auto" />
        )}
      </div>
      <figcaption className="mt-2.5 text-center text-xs text-[var(--text-muted)]">{alt}</figcaption>
    </figure>
  );
}
