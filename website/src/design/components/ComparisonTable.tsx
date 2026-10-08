import React from 'react';

interface Row {
  feature: string;
  podbox: string;
  distrobox: string;
  rawPodman: string;
}

// Kept deliberately short: the rows are the differences a reader actually
// weighs when choosing, not every integration podbox happens to support.
// Screen/sound/GPU and notifications/clipboard each collapse what used to be
// four near-identical rows. Cells are terse because four columns of
// monospace in this width only fit about twenty characters before wrapping.
const COMPARISON_ROWS: Row[] = [
  {
    feature: 'Your files',
    podbox: 'Isolated by default',
    distrobox: 'Whole $HOME mounted',
    rawPodman: 'Manual -v flags',
  },
  {
    feature: 'Config',
    podbox: 'One TOML file',
    distrobox: 'Command-line flags',
    rawPodman: 'Flags every run',
  },
  {
    feature: 'Lifecycle',
    podbox: 'systemd units',
    distrobox: 'Wrapper scripts',
    rawPodman: 'You start it',
  },
  {
    feature: 'Desktop bus',
    podbox: 'Filtered proxy',
    distrobox: 'Whole session bus',
    rawPodman: 'Whole session bus',
  },
  {
    feature: 'Screen, sound, GPU',
    podbox: 'Shared, GPU optional',
    distrobox: 'Shared by default',
    rawPodman: 'Manual device flags',
  },
  {
    feature: 'Alerts, clipboard',
    podbox: 'Works, no shared bus',
    distrobox: 'Via the shared bus',
    rawPodman: 'Not supported',
  },
  {
    feature: 'Host commands',
    podbox: 'Opt-in and filtered',
    distrobox: 'distrobox-host-exec',
    rawPodman: 'Not supported',
  },
  {
    feature: 'Built-in packages',
    podbox: 'Baked into the image',
    distrobox: 'Reinstalled each build',
    rawPodman: 'n/a',
  },
  {
    feature: 'Reproducible',
    podbox: 'Yes, from the file',
    distrobox: 'Partly, image only',
    rawPodman: 'No',
  },
  {
    feature: 'Runtimes',
    podbox: 'Podman only',
    distrobox: 'Podman, Docker',
    rawPodman: 'Any',
  },
];

// The widest cell decides how wide the "What" column has to be, so measure it
// rather than guessing: table layout then gives the rest of the space to the
// podbox column and every cell lands on one line.

export function ComparisonTable() {
  return (
    <div className="w-full">
      <div className="overflow-x-auto rounded-[3px] border border-[var(--border)]">
        <table className="w-full min-w-[720px] text-left border-collapse text-xs font-mono [&_td]:whitespace-nowrap [&_th]:whitespace-nowrap">
          <caption className="sr-only">Comparison of podbox, Distrobox/Toolbox, and raw Podman capabilities</caption>
          <thead>
            <tr>
              <th scope="col" className="p-3.5 text-[var(--text-muted)] font-medium w-1/5">
                What
              </th>
              <th scope="col" className="p-3.5 font-bold text-[var(--accent-mauve)] w-2/5">
                podbox
              </th>
              <th scope="col" className="p-3.5 text-[var(--text-subtext)] font-medium w-1/5">
                Distrobox
              </th>
              <th scope="col" className="p-3.5 text-[var(--text-muted)] font-medium w-1/5">
                Raw podman
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--border)]">
            {COMPARISON_ROWS.map((row) => (
              <tr key={row.feature} className="hover:bg-[var(--bg-surface0)]/30 transition-colors">
                <th scope="row" className="p-3.5 text-left font-normal text-[var(--text-muted)]">
                  {row.feature}
                </th>
                <td className="p-3.5 text-[var(--text-primary)]">{row.podbox}</td>
                <td className="p-3.5 text-[var(--text-subtext)]">{row.distrobox}</td>
                <td className="p-3.5 text-[var(--text-muted)]">{row.rawPodman}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <p className="mt-3 text-xs text-[var(--text-muted)] leading-relaxed">
        Both projects are reasonable. Distrobox optimises for feeling like the host; podbox optimises for
        reproducing the same environment from a file.
      </p>
    </div>
  );
}