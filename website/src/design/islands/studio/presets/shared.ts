import type { LucideIcon } from 'lucide-react';
import type { StudioValues } from '../useStudioState';

export type PresetId = 'rust' | 'flutter' | 'arch-gui' | 'fullstack' | 'gaming' | 'minimal';

export interface PresetDef {
  id: PresetId;
  label: string;
  /** A few words on what this preset is for, shown in the menu. */
  blurb: string;
  icon: LucideIcon;
  /** Tailwind text colour for the icon. */
  accent: string;
  /** Only what makes this preset different. */
  overrides: Partial<StudioValues>;
}

/** Container name drives the image tag and the home path; keep them in sync. */
export const named = (name: string) =>
  ({ containerName: name, imageName: name, containerHome: `~/containers/${name}` }) satisfies Partial<StudioValues>;

export const desktopIntegration = {
  intWayland: true,
  intAudio: true,
  intGpu: 'auto',
} satisfies Partial<StudioValues>;

export const headlessIntegration = {
  intWayland: false,
  intAudio: false,
  intGpu: 'false',
} satisfies Partial<StudioValues>;
