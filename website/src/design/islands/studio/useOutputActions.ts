import type { StudioState } from './useStudioState';

type OutputState = Pick<StudioState, 'activeView' | 'containerName' | 'copied' | 'setCopied' | 'setShowExportMenu'>;

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

const handleDownloadQuadlet = () => {
  const quadletContent = quadlet;
  const blob = new Blob([quadletContent], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = `${st.containerName || 'podbox'}.container`;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
  st.setShowExportMenu(false);
};

const handleDownloadContainerfile = () => {
  if (!containerfile) return;
  const blob = new Blob([containerfile], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = 'Containerfile';
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


  return { handleDownloadToml, handleDownloadQuadlet, handleDownloadContainerfile, handleCopyConfig };
}
