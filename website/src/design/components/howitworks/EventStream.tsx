import React from 'react';
import { TerminalCodeBlock } from '../TerminalCodeBlock';
import type { NodeDetail } from './nodeData';

export function EventStream({ node, actionLog }: { node: NodeDetail; actionLog: string[] }) {
  return (
          <div className="lg:col-span-6 flex flex-col bg-[var(--bg-base)]">
            <div className="flex items-center justify-between px-3 py-2 border-b border-[var(--border)] bg-[var(--bg-crust)]/60 text-xs">
              <span className="text-[var(--text-muted)] font-medium">
                {node.code?.filename || 'Live Event Stream'}
              </span>
              <span className="text-[10px] text-[var(--accent-green)] flex items-center gap-1">
                <span className="w-2 h-2 rounded-full bg-[var(--accent-green)] animate-pulse" />
                <span>active</span>
              </span>
            </div>

            <div className="p-3 flex-1 overflow-y-auto">
              {node.code ? (
                <TerminalCodeBlock
                  code={node.code.snippet}
                  language={node.code.lang}
                  filename={node.code.filename}
                />
              ) : null}

              {/* Event / Action Log Output */}
              <div className="mt-2 p-2.5 rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-[11px] space-y-1 font-mono">
                <div className="text-[10px] text-[var(--text-muted)] uppercase tracking-widest font-bold pb-1 border-b border-[var(--border)]/50">
                  Event Stream &amp; Activity Log
                </div>
                {actionLog.map((log, lIdx) => (
                  <div
                    key={lIdx}
                    className={`leading-relaxed truncate ${
                      lIdx === 0
                        ? 'text-[var(--accent-mauve)] font-semibold'
                        : 'text-[var(--text-muted)]'
                    }`}
                  >
                    {log}
                  </div>
                ))}
              </div>
            </div>
          </div>
  );
}
