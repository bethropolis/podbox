import type { StudioState } from './useStudioState';

type OutputState = Pick<StudioState, 'activeView' | 'containerName' | 'copied' | 'setCopied' | 'setShowExportMenu'>;

/**
 * Downloads are TOML-only: the `.container` unit and the Containerfile are
 * outputs that `podbox install` regenerates from that TOML, so offering them
 * as downloads would hand you files with nowhere to put them.
 */
export function useOutputActions(st: OutputState, toml: string, quadlet: string, containerfile?: string | null) {
const handleDownloadToml = () => {
  const tomlContent = toml;
  const blob = new Blob([tomlContent], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = `${st.containerName || 'podbox'}.toml`;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
  st.setShowExportMenu(false);
};

const handleCopyConfig = async () => {
  const content = st.activeView === 'toml' ? toml : st.activeView === 'containerfile' ? (containerfile ?? '') : quadlet;
  await navigator.clipboard.writeText(content);
  st.setCopied(true);
  setTimeout(() => st.setCopied(false), 2000);
};


  return { handleDownloadToml, handleCopyConfig };
}
