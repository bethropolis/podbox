import { Lock } from 'lucide-react';
import { headlessIntegration, named, type PresetDef } from './shared';

export const minimalPreset: PresetDef = {
  id: 'minimal',
  label: 'Hardened',
  blurb: 'Offline and locked down',
  icon: Lock,
  accent: 'text-[var(--accent-red)]',
  overrides: {
    selectedPresetDistro: 'alpine:3.24',
    ...named('micro-box'),
    packagesInstallList: ['busybox-extras', 'curl', 'ca-certificates'],
    runCommands: ['apk cache clean'],
    ...headlessIntegration,
    readOnlyRootfs: true,
    noNewPrivileges: true,
    // Ports are rejected when the network is none/offline. The defaults are
    // merged underneath, so this preset can no longer inherit them from the
    // session it was applied to.
    netMode: 'none',
    portMappingsList: [],
  },
};
