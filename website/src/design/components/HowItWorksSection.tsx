import React, { useState } from 'react';
import { nodeDetails, type TabType } from './howitworks/nodeData';
import { BuildDiagram } from './howitworks/BuildDiagram';
import { RuntimeDiagram } from './howitworks/RuntimeDiagram';
import { ProtocolDiagram } from './howitworks/ProtocolDiagram';
import { NodeInspector } from './howitworks/NodeInspector';

interface HowItWorksSectionProps {
  // Navigation is plain MPA links; no callback props cross the Astro boundary.
}

const TABS: { id: TabType; label: string }[] = [
  { id: 'build', label: 'Build time' },
  { id: 'runtime', label: 'While it runs' },
  { id: 'protocol', label: 'Messages' },
];

const NODE_PREFIX: Record<TabType, string> = {
  build: 'build_',
  runtime: 'rt_',
  protocol: 'proto_',
};

const DEFAULT_NODE: Record<TabType, string> = {
  build: 'build_codegen',
  runtime: 'rt_guest',
  protocol: 'proto_handshake',
};

export function HowItWorksSection(_props: HowItWorksSectionProps) {
  const [activeTab, setActiveTab] = useState<TabType>('build');
  const [selectedNode, setSelectedNode] = useState<string>(DEFAULT_NODE.build);

  const handleTabChange = (tab: TabType) => {
    setActiveTab(tab);
    setSelectedNode(DEFAULT_NODE[tab]);
  };

  const currentNode = nodeDetails[selectedNode] || nodeDetails.build_codegen;

  return (
    <section id="architecture" className="w-full scroll-mt-20">
      <header className="mb-10 max-w-2xl">
        <h2 className="text-2xl sm:text-[2rem] font-semibold tracking-[-0.025em] leading-[1.15] text-[var(--text-primary)]">
          How it works
        </h2>
        <p className="mt-3 text-[15px] sm:text-base leading-relaxed text-[var(--text-subtext)]">
          Two moments matter: when you build, and while the container runs. Select a box to see what
          that part does.
        </p>
      </header>

      <div className="rounded-md border border-[var(--border)] bg-[var(--bg-mantle)] overflow-hidden">
        <div role="tablist" aria-label="Diagram" className="flex gap-1 overflow-x-auto scrollbar-none border-b border-[var(--border)] px-3 pt-3">
          {TABS.map((t) => (
            <button
              key={t.id}
              role="tab"
              aria-selected={activeTab === t.id}
              onClick={() => handleTabChange(t.id)}
              type="button"
              className={`-mb-px whitespace-nowrap border-b-2 px-3 pb-2.5 pt-1 text-sm transition-colors cursor-pointer ${
                activeTab === t.id
                  ? 'border-[var(--accent-mauve)] text-[var(--text-primary)] font-medium'
                  : 'border-transparent text-[var(--text-muted)] hover:text-[var(--text-primary)]'
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>

        {/* SVG CANVAS */}
        {/* Phones: the diagram stays legible by scrolling sideways instead of
            shrinking, and the parts are also listed as buttons so they can be
            picked without panning around. */}
        <div className="sm:hidden flex gap-1.5 overflow-x-auto scrollbar-none px-3 py-3 border-b border-[var(--border)]">
          {Object.entries(nodeDetails)
            .filter(([id]) => id.startsWith(NODE_PREFIX[activeTab]))
            .map(([id, node]) => (
              <button
                key={id}
                type="button"
                onClick={() => setSelectedNode(id)}
                aria-pressed={selectedNode === id}
                className={`shrink-0 rounded border px-2.5 py-1 font-mono text-xs transition-colors ${
                  selectedNode === id
                    ? 'border-[var(--accent-mauve)] text-[var(--text-primary)] bg-[var(--bg-surface0)]'
                    : 'border-[var(--border)] text-[var(--text-subtext)]'
                }`}
              >
                {node.title}
              </button>
            ))}
        </div>

        <div className="p-4 sm:p-8 bg-[var(--bg-base)] flex items-center justify-center-safe relative overflow-x-auto min-h-[260px] sm:min-h-[340px] [&>svg]:min-w-[620px] sm:[&>svg]:min-w-0">

          {activeTab === 'build' && (
            <BuildDiagram selectedNode={selectedNode} onSelect={setSelectedNode} />
          )}

          {activeTab === 'runtime' && (
            <RuntimeDiagram selectedNode={selectedNode} onSelect={setSelectedNode} />
          )}

          {activeTab === 'protocol' && (
            <ProtocolDiagram selectedNode={selectedNode} onSelect={setSelectedNode} />
          )}
        </div>

        {/* INSPECTOR PANEL */}
        <div className="border-t border-[var(--border)] bg-[var(--bg-mantle)]">
          <NodeInspector node={currentNode} />
        </div>
      </div>
    </section>
  );
}
