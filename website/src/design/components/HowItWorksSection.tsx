import React, { useState } from 'react';
import {
  FileCode,
  Shield,
  Radio,
} from 'lucide-react';
import { nodeDetails, type TabType } from './howitworks/nodeData';
import { BuildDiagram } from './howitworks/BuildDiagram';
import { RuntimeDiagram } from './howitworks/RuntimeDiagram';
import { ProtocolDiagram } from './howitworks/ProtocolDiagram';
import { NodeInspector } from './howitworks/NodeInspector';

interface HowItWorksSectionProps {
  // Navigation is plain MPA links; no callback props cross the Astro boundary.
}

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
    <section id="architecture" className="w-full my-12 font-mono">
      {/* Section Header */}
      <div className="mb-8">
        <div className="text-xs text-[var(--accent-blue)] uppercase tracking-wider font-bold mb-2">
          how it works
        </div>
        <div>
          <h2 className="text-2xl sm:text-3xl font-bold tracking-tight text-[var(--text-primary)]">
            How it works
          </h2>
          <p className="mt-2 text-sm text-[var(--text-subtext)] max-w-2xl leading-relaxed">
            Click any box to see what that part does.
          </p>
        </div>
      </div>

      {/* Main Visualizer Container */}
      <div className="rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)] overflow-hidden">
        {/* View switcher. No fake window chrome: a decorative title bar above a
            diagram the reader is meant to look at, not a terminal. */}
        <div className="flex flex-wrap items-center gap-2 px-4 py-3 border-b border-[var(--border)]">
          <div className="inline-flex rounded-[2px] p-0.5 bg-[var(--bg-base)] border border-[var(--border)] text-xs">
            <button
              onClick={() => handleTabChange('build')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'build'
                  ? 'bg-[var(--accent-mauve)] text-[#11111b] font-bold'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <FileCode className="w-3.5 h-3.5" />
              <span>Build time</span>
            </button>
            <button
              onClick={() => handleTabChange('runtime')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'runtime'
                  ? 'bg-[var(--accent-blue)] text-[#11111b] font-bold'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <Shield className="w-3.5 h-3.5" />
              <span>While it runs</span>
            </button>
            <button
              onClick={() => handleTabChange('protocol')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'protocol'
                  ? 'bg-[var(--accent-peach)] text-[#11111b] font-bold'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <Radio className="w-3.5 h-3.5" />
              <span>Messages</span>
            </button>
          </div>
        </div>

        {/* SVG CANVAS */}
        <div className="p-4 sm:p-6 bg-[var(--bg-base)] flex items-center justify-center relative overflow-x-auto min-h-[340px]">
          {/* Subtle dot background */}
          <div className="absolute inset-0 bg-terminal-dots pointer-events-none" />

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
