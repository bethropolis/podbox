import React from 'react';
import {
  Play,
  Pause,
  RotateCcw,
  Cpu,
  FileCode,
  Shield,
  Radio,
} from 'lucide-react';
import { nodeDetails } from './howitworks/nodeData';
import { useSimulation } from './howitworks/useSimulation';
import { BuildDiagram } from './howitworks/BuildDiagram';
import { RuntimeDiagram } from './howitworks/RuntimeDiagram';
import { ProtocolDiagram } from './howitworks/ProtocolDiagram';
import { RuntimeTriggerBar } from './howitworks/RuntimeTriggerBar';
import { NodeInspector } from './howitworks/NodeInspector';
import { EventStream } from './howitworks/EventStream';

interface HowItWorksSectionProps {
  // Navigation is plain MPA links; no callback props cross the Astro boundary.
}

export function HowItWorksSection(_props: HowItWorksSectionProps) {
  const {
    activeTab, selectedNode, isSimulating, simStep, activePacket, actionLog,
    handleTabChange, triggerRuntimeAction, setSelectedNode, setIsSimulating, setSimStep, setActionLog,
  } = useSimulation();

  const currentNode = nodeDetails[selectedNode] || nodeDetails.build_codegen;

  return (
    <section id="architecture" className="w-full my-12 font-mono">
      {/* Section Header */}
      <div className="mb-8">
        <div className="flex items-center gap-2 text-xs text-[var(--accent-blue)] uppercase tracking-wider font-bold mb-2">
          <Cpu className="w-4 h-4" />
          <span>internals // interactive-architecture-visualizer</span>
        </div>
        <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
          <div>
            <h2 className="text-2xl sm:text-3xl font-bold tracking-tight text-[var(--text-primary)]">
              How podbox works: Build-time & Runtime
            </h2>
            <p className="mt-2 text-sm text-[var(--text-subtext)] max-w-2xl leading-relaxed">
              Explore the lifecycle stages below. Click any node to inspect its internal mechanics,
              or run the interactive simulation to trace data packets as they cross security boundaries.
            </p>
          </div>

          {/* Action Simulation Controls */}
          <div className="flex items-center gap-2 bg-[var(--bg-mantle)] p-1.5 rounded-[4px] border border-[var(--border)] shrink-0">
            <button
              onClick={() => setIsSimulating(!isSimulating)}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono rounded-[2px] transition-colors cursor-pointer font-bold ${
                isSimulating
                  ? 'bg-[var(--accent-red)] text-[#11111b]'
                  : 'bg-[var(--accent-green)] text-[#11111b]'
              }`}
            >
              {isSimulating ? (
                <>
                  <Pause className="w-3.5 h-3.5" />
                  <span>Pause Simulation</span>
                </>
              ) : (
                <>
                  <Play className="w-3.5 h-3.5" />
                  <span>Trace Pipeline</span>
                </>
              )}
            </button>

            <button
              onClick={() => {
                setIsSimulating(false);
                setSimStep(0);
                setActionLog(['Simulation reset.']);
              }}
              type="button"
              className="p-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] rounded-[2px] cursor-pointer"
              title="Reset simulation"
            >
              <RotateCcw className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>

      {/* Main Visualizer Container */}
      <div className="rounded-[4px] border border-[var(--border)] bg-[var(--bg-mantle)] overflow-hidden shadow-lg">
        {/* Terminal Chrome Bar with View Switchers */}
        <div className="flex flex-wrap items-center justify-between px-4 py-2.5 bg-[var(--bg-crust)] border-b border-[var(--border)] gap-2">
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-1.5">
              <span className="w-2.5 h-2.5 rounded-full bg-[#f38ba8]/80 inline-block" />
              <span className="w-2.5 h-2.5 rounded-full bg-[#f9e2af]/80 inline-block" />
              <span className="w-2.5 h-2.5 rounded-full bg-[#a6e3a1]/80 inline-block" />
            </div>
            <span className="text-xs text-[var(--text-subtext)] font-semibold uppercase tracking-wider">
              arch-pipeline // {activeTab}.svg
            </span>
          </div>

          {/* View Tab Buttons */}
          <div className="inline-flex rounded-[2px] p-0.5 bg-[var(--bg-base)] border border-[var(--border)] text-xs">
            <button
              onClick={() => handleTabChange('build')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1 rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'build'
                  ? 'bg-[var(--accent-mauve)] text-[#11111b] font-bold'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <FileCode className="w-3.5 h-3.5" />
              <span>Phase 1: Build-Time</span>
            </button>
            <button
              onClick={() => handleTabChange('runtime')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1 rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'runtime'
                  ? 'bg-[var(--accent-blue)] text-[#11111b] font-bold'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <Shield className="w-3.5 h-3.5" />
              <span>Phase 2: Runtime IPC</span>
            </button>
            <button
              onClick={() => handleTabChange('protocol')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1 rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'protocol'
                  ? 'bg-[var(--accent-peach)] text-[#11111b] font-bold'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <Radio className="w-3.5 h-3.5" />
              <span>Socket Protocol</span>
            </button>
          </div>
        </div>

        {/* Interactive Event Dispatch Bar for Runtime Mode */}
        {activeTab === 'runtime' && (
          <RuntimeTriggerBar onTrigger={triggerRuntimeAction} />
        )}

        {/* SVG INTERACTIVE CANVAS */}
        <div className="p-4 sm:p-6 bg-[var(--bg-base)] flex items-center justify-center relative overflow-x-auto min-h-[340px]">
          {/* Subtle dot background */}
          <div className="absolute inset-0 bg-terminal-dots pointer-events-none" />

          {activeTab === 'build' && (
            <BuildDiagram selectedNode={selectedNode} onSelect={setSelectedNode} isSimulating={isSimulating} simStep={simStep} />
          )}

          {activeTab === 'runtime' && (
            <RuntimeDiagram selectedNode={selectedNode} onSelect={setSelectedNode} activePacket={activePacket} />
          )}

          {activeTab === 'protocol' && (
            <ProtocolDiagram selectedNode={selectedNode} onSelect={setSelectedNode} />
          )}
        </div>

        {/* INTERACTIVE INSPECTOR PANEL */}
        <div className="grid grid-cols-1 lg:grid-cols-12 border-t border-[var(--border)] divide-y lg:divide-y-0 lg:divide-x divide-[var(--border)] bg-[var(--bg-mantle)]">
          <NodeInspector node={currentNode} />
          <EventStream node={currentNode} actionLog={actionLog} />
        </div>
      </div>
    </section>
  );
}

