export interface SearchResult {
  title: string;
  category: string;
  path: string;
  snippet?: string;
  badge?: string;
}

// Anchors below are real heading slugs in the current docs (verified against
// the MarkdownRenderer slug algorithm). Keep them in sync when docs change.
export const predefinedCommands: SearchResult[] = [
  { title: 'podbox build', category: 'CLI Command', path: '/docs/getting-started#lifecycle-management', snippet: 'Build the container image from the TOML config', badge: 'build' },
  { title: 'podbox create <name>', category: 'CLI Command', path: '/docs/getting-started#prebuilt-quick-start', snippet: 'Init, build, enable and start in one step', badge: 'create' },
  { title: 'podbox enter [<name>]', category: 'CLI Command', path: '/docs/getting-started#open-a-shell', snippet: 'Open an interactive shell in the container', badge: 'enter' },
  { title: 'podbox exec -- <cmd>', category: 'CLI Command', path: '/docs/getting-started#run-commands', snippet: 'Execute a command directly inside the container', badge: 'exec' },
  { title: 'podbox list', category: 'CLI Command', path: '/docs/cli#command-groups', snippet: 'List all podbox-managed containers', badge: 'list' },
  { title: 'podbox start / stop', category: 'CLI Command', path: '/docs/getting-started#start-and-stop', snippet: 'Start or stop a container via systemd', badge: 'lifecycle' },
  { title: 'podbox export app <name>', category: 'CLI Command', path: '/docs/export#app-export', snippet: 'Export a .desktop file to the host launcher', badge: 'export' },
  { title: 'podbox export bin <name>', category: 'CLI Command', path: '/docs/export#binary-export', snippet: 'Create a binary shim in ~/.local/bin', badge: 'export' },
  { title: 'podbox snapshot / restore', category: 'CLI Command', path: '/docs/getting-started#snapshots', snippet: 'Commit container state and roll back', badge: 'snapshot' },
  { title: 'podbox remove', category: 'CLI Command', path: '/docs/getting-started#remove', snippet: 'Remove the container, home, or config', badge: 'remove' },
  { title: '[container] config table', category: 'Configuration', path: '/docs/config#container', snippet: 'Container name, home, shell, and limits', badge: 'config' },
  { title: '[image] config table', category: 'Configuration', path: '/docs/config#image', snippet: 'Base image, packages, and RUN commands', badge: 'config' },
  { title: '[integration] keys', category: 'Configuration', path: '/docs/config#integration', snippet: 'Wayland, audio, GPU, D-Bus, and mounts', badge: 'config' },
  { title: '[dbus] proxy rules', category: 'Configuration', path: '/docs/dbus-proxy', snippet: 'xdg-dbus-proxy filtering and presets', badge: 'dbus' },
  { title: 'podbox Studio (Config Workbench)', category: 'Configuration', path: '/studio', snippet: 'Interactive container specification architect and Quadlet synthesizer', badge: 'studio' },
  { title: 'Guest Interceptor Protocol', category: 'Architecture', path: '/docs/protocol', snippet: 'UNIX domain socket JSON communication between guest and host', badge: 'protocol' },
  { title: 'Baked-in Packages Matrix', category: 'Documentation', path: '/docs/baked-in-packages', snippet: 'Pre-installed base utilities per distro family', badge: 'packages' },
];
