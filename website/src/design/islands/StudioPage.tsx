import React from 'react';
import { useStudioState } from './studio/useStudioState';
import { generateStudioToml } from './studio/generateToml';
import { generateStudioQuadlet } from './studio/generateQuadlet';
import { StudioHeader } from './studio/StudioHeader';
import { CategoryTabs } from './studio/CategoryTabs';
import { ImagePanel } from './studio/panels/ImagePanel';
import { ContainerPanel } from './studio/panels/ContainerPanel';
import { SecurityPanel } from './studio/panels/SecurityPanel';
import { NetworkPanel } from './studio/panels/NetworkPanel';
import { IntegrationPanel } from './studio/panels/IntegrationPanel';
import { LifecyclePanel } from './studio/panels/LifecyclePanel';
import { DbusPanel } from './studio/panels/DbusPanel';
import { WaylandPanel } from './studio/panels/WaylandPanel';
import { CodePreview } from './studio/CodePreview';

interface StudioPageProps {
  // Navigation is plain MPA links; no callback props cross the Astro boundary.
}

export function StudioPage(_props: StudioPageProps) {
  const st = useStudioState();
  const { isFullscreen, activeCategory } = st;
  const toml = generateStudioToml(st.values);
  const quadlet = generateStudioQuadlet(st.values);

  return (
<div
  className={`w-full font-sans text-[var(--text-primary)] transition-all ${
    st.isFullscreen
      ? 'fixed inset-0 z-50 bg-[var(--bg-base)] flex flex-col p-3 sm:p-4 h-screen max-h-screen overflow-hidden'
      : 'max-w-7xl mx-auto px-4 sm:px-6 py-4 sm:py-6 lg:h-[calc(100vh-4.5rem)] lg:flex lg:flex-col lg:overflow-hidden'
  }`}
>
  {/* -------------------------------------------------------------------- */}
      <StudioHeader st={st} toml={toml} quadlet={quadlet} />

{/* 2. MAIN WORKSPACE (SETTINGS + LIVE PREVIEW)                           */}
{/* -------------------------------------------------------------------- */}
<div className={`grid grid-cols-1 lg:grid-cols-12 gap-4 lg:items-stretch ${st.isFullscreen ? 'flex-1 min-h-0 overflow-hidden' : 'lg:flex-1 lg:min-h-0 lg:overflow-hidden'}`}>
  {/* LEFT COLUMN: Categories & Settings Form (7 cols) */}
  <div className="lg:col-span-7 flex flex-col min-h-0 h-full overflow-hidden rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)]/40 shadow-sm">
          <CategoryTabs st={st} />

{/* Form Content Body - Scrollable */}
<div className="flex-1 min-h-0 overflow-y-auto p-4 sm:p-5 space-y-6 custom-scrollbar bg-[var(--bg-base)]">
            {activeCategory === 'image' && <ImagePanel st={st} />}
            {activeCategory === 'container' && <ContainerPanel st={st} />}
            {activeCategory === 'security' && <SecurityPanel st={st} />}
            {activeCategory === 'network' && <NetworkPanel st={st} />}
            {activeCategory === 'integration' && <IntegrationPanel st={st} />}
            {activeCategory === 'lifecycle' && <LifecyclePanel st={st} />}
            {activeCategory === 'dbus' && <DbusPanel st={st} />}
            {activeCategory === 'wayland' && <WaylandPanel st={st} />}
          </div>
        </div>

        <CodePreview st={st} toml={toml} quadlet={quadlet} />
      </div>
    </div>
  );
}
