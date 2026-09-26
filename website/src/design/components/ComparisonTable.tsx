import React from 'react';
import { Check, X, Shield, Cpu, RefreshCw, Layers } from 'lucide-react';

interface Row {
  feature: string;
  podbox: string;
  podboxHighlight?: 'good' | 'neutral' | 'warn';
  distrobox: string;
  distroboxHighlight?: 'good' | 'neutral' | 'warn';
  rawPodman: string;
  rawPodmanHighlight?: 'good' | 'neutral' | 'warn';
}

const COMPARISON_ROWS: Row[] = [
  {
    feature: 'Home directory',
    podbox: 'Isolated volume, opt-in sharing',
    podboxHighlight: 'good',
    distrobox: 'Full $HOME mounted by default',
    distroboxHighlight: 'warn',
    rawPodman: 'Manual -v flags',
    rawPodmanHighlight: 'neutral',
  },
  {
    feature: 'Config',
    podbox: 'Declarative TOML, version-controllable',
    podboxHighlight: 'good',
    distrobox: 'Imperative CLI flags',
    distroboxHighlight: 'neutral',
    rawPodman: 'Shell flags per run',
    rawPodmanHighlight: 'warn',
  },
  {
    feature: 'Lifecycle',
    podbox: 'systemd Quadlet units',
    podboxHighlight: 'good',
    distrobox: 'Shell shims',
    distroboxHighlight: 'neutral',
    rawPodman: 'Manual',
    rawPodmanHighlight: 'warn',
  },
  {
    feature: 'D-Bus',
    podbox: 'Filtered via xdg-dbus-proxy',
    podboxHighlight: 'good',
    distrobox: 'Unfiltered session bus',
    distroboxHighlight: 'warn',
    rawPodman: 'Unfiltered',
    rawPodmanHighlight: 'warn',
  },
  {
    feature: 'Wayland / audio',
    podbox: 'Opt-out (on by default)',
    podboxHighlight: 'good',
    distrobox: 'Always on',
    distroboxHighlight: 'neutral',
    rawPodman: 'Manual',
    rawPodmanHighlight: 'warn',
  },
  {
    feature: 'GPU',
    podbox: 'auto / nvidia / off',
    podboxHighlight: 'good',
    distrobox: '--nvidia flag',
    distroboxHighlight: 'neutral',
    rawPodman: 'Manual device flags',
    rawPodmanHighlight: 'neutral',
  },
  {
    feature: 'Notifications',
    podbox: 'Guest interceptor → host',
    podboxHighlight: 'good',
    distrobox: 'Via shared D-Bus',
    distroboxHighlight: 'neutral',
    rawPodman: 'Not supported',
    rawPodmanHighlight: 'warn',
  },
  {
    feature: 'Clipboard',
    podbox: 'Guest interceptor → host',
    podboxHighlight: 'good',
    distrobox: 'Via shared home',
    distroboxHighlight: 'neutral',
    rawPodman: 'Not supported',
    rawPodmanHighlight: 'warn',
  },
  {
    feature: 'Host commands',
    podbox: 'host-exec interceptor',
    podboxHighlight: 'good',
    distrobox: 'distrobox-host-exec',
    distroboxHighlight: 'neutral',
    rawPodman: 'Not supported',
    rawPodmanHighlight: 'warn',
  },
  {
    feature: 'SSH agent',
    podbox: 'Socket forward (opt-in)',
    podboxHighlight: 'good',
    distrobox: 'Auto-mounted',
    distroboxHighlight: 'warn',
    rawPodman: 'Not supported',
    rawPodmanHighlight: 'warn',
  },
  {
    feature: 'Baked images',
    podbox: 'Yes: packages in image, not runtime',
    podboxHighlight: 'good',
    distrobox: 'No: packages reinstalled on rebuild',
    distroboxHighlight: 'warn',
    rawPodman: 'N/A',
    rawPodmanHighlight: 'neutral',
  },
  {
    feature: 'Reproducibility',
    podbox: 'Full: TOML → image → unit',
    podboxHighlight: 'good',
    distrobox: 'Partial: image only',
    distroboxHighlight: 'neutral',
    rawPodman: 'None',
    rawPodmanHighlight: 'warn',
  },
  {
    feature: 'Runtime',
    podbox: 'Podman only (tight integration)',
    podboxHighlight: 'neutral',
    distrobox: 'Podman / Docker / lilipod',
    distroboxHighlight: 'neutral',
    rawPodman: 'Any OCI runtime',
    rawPodmanHighlight: 'neutral',
  },
];

export function ComparisonTable() {
  return (
    <div className="w-full my-6 overflow-hidden rounded-[4px] border border-[var(--border)] bg-[var(--bg-mantle)]">
      {/* Table Header / Window Bar */}
      <div className="flex items-center justify-between px-4 py-2.5 bg-[var(--bg-crust)] border-b border-[var(--border)]">
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1.5" aria-hidden="true">
            <span className="w-2.5 h-2.5 rounded-full bg-[#f38ba8]/80 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-[#f9e2af]/80 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-[#a6e3a1]/80 inline-block" />
          </div>
          <span className="text-xs text-[var(--text-subtext)] font-semibold uppercase tracking-wider ml-1">
            matrix // comparison.table
          </span>
        </div>
        <div className="text-[11px] text-[var(--accent-mauve)] font-medium">
          declarative vs imperative
        </div>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full text-left border-collapse text-xs md:text-sm font-mono">
          <caption className="sr-only">Comparison of podbox, Distrobox/Toolbox, and raw Podman capabilities</caption>
          <thead>
            <tr className="border-b border-[var(--border)] bg-[var(--bg-base)]">
              <th scope="col" className="p-3 text-[var(--text-muted)] font-semibold w-1/4">
                Capability
              </th>
              <th scope="col" className="p-3 bg-[var(--accent-mauve)]/10 text-[var(--accent-mauve)] font-bold border-x border-[var(--border)] w-1/3">
                <div className="flex items-center gap-1.5">
                  <span className="inline-block w-2 h-2 rounded-full bg-[var(--accent-mauve)] animate-pulse" aria-hidden="true" />
                  podbox
                </div>
              </th>
              <th scope="col" className="p-3 text-[var(--text-primary)] font-semibold w-1/4">
                Distrobox / Toolbox
              </th>
              <th scope="col" className="p-3 text-[var(--text-muted)] font-semibold w-1/6">
                Raw podman run
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--border)]">
            {COMPARISON_ROWS.map((row, idx) => (
              <tr
                key={idx}
                className="hover:bg-[var(--bg-surface0)]/40 transition-colors"
              >
                <th scope="row" className="p-3 font-medium text-[var(--text-primary)] text-left font-normal">
                  {row.feature}
                </th>
                <td className="p-3 font-semibold text-[var(--text-primary)] bg-[var(--accent-mauve)]/5 border-x border-[var(--border)]">
                  <div className="flex items-start gap-1.5">
                    {row.podboxHighlight === 'good' && (
                      <Check className="w-3.5 h-3.5 text-[var(--accent-green)] shrink-0 mt-0.5" />
                    )}
                    <span>{row.podbox}</span>
                  </div>
                </td>
                <td className="p-3 text-[var(--text-subtext)]">
                  <div className="flex items-start gap-1.5">
                    {row.distroboxHighlight === 'warn' && (
                      <span className="text-[var(--accent-peach)] shrink-0 font-bold">!</span>
                    )}
                    <span>{row.distrobox}</span>
                  </div>
                </td>
                <td className="p-3 text-[var(--text-muted)]">
                  <div className="flex items-start gap-1.5">
                    {row.rawPodmanHighlight === 'warn' && (
                      <X className="w-3.5 h-3.5 text-[var(--accent-red)] shrink-0 mt-0.5" />
                    )}
                    <span>{row.rawPodman}</span>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="p-3 bg-[var(--bg-crust)] border-t border-[var(--border)] text-[11px] text-[var(--text-subtext)] flex flex-col sm:flex-row items-start sm:items-center justify-between gap-2">
        <span>* Distrobox prioritizes host transparency; podbox prioritizes isolation, reproducibility & systemd Quadlets.</span>
        <span className="text-[var(--accent-blue)]">Official README Specification</span>
      </div>
    </div>
  );
}
