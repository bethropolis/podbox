import { Globe } from 'lucide-react';
import { headlessIntegration, named, type PresetDef } from './shared';

export const fullstackPreset: PresetDef = {
  id: 'fullstack',
  label: 'Full-Stack',
  blurb: 'Web stack, open ports',
  icon: Globe,
  accent: 'text-[var(--accent-green)]',
  overrides: {
    selectedPresetDistro: 'ubuntu:24.04',
    ...named('fullstack-web'),
    packagesInstallList: ['nodejs', 'npm', 'pnpm', 'git', 'curl', 'python3'],
    runCommands: ['apt-get clean', 'rm -rf /var/lib/apt/lists/*'],
    portMappingsList: ['3000:3000', '5173:5173', '8080:8080'],
    netMode: 'pasta',
    ...headlessIntegration,
    lifeAutostart: true,
  },
};
