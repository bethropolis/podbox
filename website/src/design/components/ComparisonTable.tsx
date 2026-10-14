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

const PODBOX_COL = 'bg-[color-mix(in_srgb,var(--accent-mauve)_7%,transparent)]';

export function ComparisonTable() {
  return (
    <div className="w-full">
      {/* Phones: four columns can't fit, so each aspect becomes a short stack
          with podbox first and the alternatives underneath. */}
      <dl className="md:hidden divide-y divide-[var(--border)] rounded-md border border-[var(--border)]">
        {COMPARISON_ROWS.map((row) => (
          <div key={row.feature} className="px-4 py-3.5">
            <dt className="text-xs text-[var(--text-muted)]">{row.feature}</dt>
            <dd className="mt-1 text-[15px] font-medium text-[var(--text-primary)]">{row.podbox}</dd>
            <dd className="mt-1.5 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-0.5 text-[13px]">
              <span className="text-[var(--text-muted)]">Distrobox</span>
              <span className="text-[var(--text-subtext)]">{row.distrobox}</span>
              <span className="text-[var(--text-muted)]">podman</span>
              <span className="text-[var(--text-subtext)]">{row.rawPodman}</span>
            </dd>
          </div>
        ))}
      </dl>

      <div className="hidden md:block overflow-x-auto rounded-md border border-[var(--border)]">
        <table className="w-full text-left border-collapse text-[13px] [&_td]:whitespace-nowrap [&_th]:whitespace-nowrap">
          <caption className="sr-only">Comparison of podbox, Distrobox/Toolbox, and raw Podman capabilities</caption>
          <thead className="bg-[var(--bg-mantle)]">
            <tr className="border-b border-[var(--border)]">
              <th scope="col" className="w-1/5 px-4 py-3">
                <span className="sr-only">Aspect</span>
              </th>
              <th scope="col" className={`w-2/5 px-4 py-3 font-semibold text-[var(--accent-mauve)] ${PODBOX_COL}`}>
                podbox
              </th>
              <th scope="col" className="w-1/5 px-4 py-3 font-medium text-[var(--text-subtext)]">
                Distrobox
              </th>
              <th scope="col" className="w-1/5 px-4 py-3 font-medium text-[var(--text-subtext)]">
                Raw podman
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--border)]">
            {COMPARISON_ROWS.map((row) => (
              <tr key={row.feature}>
                <th scope="row" className="px-4 py-3 text-left font-normal text-[var(--text-muted)]">
                  {row.feature}
                </th>
                <td className={`px-4 py-3 text-[var(--text-primary)] ${PODBOX_COL}`}>{row.podbox}</td>
                <td className="px-4 py-3 text-[var(--text-subtext)]">{row.distrobox}</td>
                <td className="px-4 py-3 text-[var(--text-muted)]">{row.rawPodman}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <p className="mt-4 max-w-2xl text-sm text-[var(--text-muted)] leading-relaxed">
        Both are reasonable choices. Distrobox optimises for feeling like the host; podbox optimises for
        reproducing the same environment from a file.
      </p>
    </div>
  );
}
