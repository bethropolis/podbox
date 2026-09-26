import { useState, useEffect } from 'react';
import type { TabType } from './nodeData';

export type RuntimeAction = 'notification' | 'clipboard' | 'url' | 'exec';

export function useSimulation() {
const [activeTab, setActiveTab] = useState<TabType>('build');
const [selectedNode, setSelectedNode] = useState<string>('build_codegen');
const [isSimulating, setIsSimulating] = useState(false);
const [simStep, setSimStep] = useState(0);
const [activePacket, setActivePacket] = useState<string | null>(null);
const [actionLog, setActionLog] = useState<string[]>([
  'System ready. Select a node or trigger an interactive simulation.',
]);

useEffect(() => {
  let timer: any;
  if (isSimulating) {
    timer = setInterval(() => {
      setSimStep(prev => {
        const next = (prev + 1) % 4;
        if (activeTab === 'build') {
          const nodes = ['build_toml', 'build_codegen', 'build_image', 'build_systemd'];
          setSelectedNode(nodes[next]);
          const msgs = [
            'Parsing and validating podbox.toml declarative schema...',
            'Running pure Rust codegen: multi-stage Containerfile & Quadlet units...',
            'Building OCI image with baked packages & guest daemon...',
            'systemd --user reloads units and claims container lifecycle supervision.',
          ];
          setActionLog(l => [msgs[next], ...l.slice(0, 5)]);
        } else if (activeTab === 'runtime') {
          const nodes = ['rt_guest', 'rt_handshake', 'rt_proxies', 'rt_host'];
          setSelectedNode(nodes[next]);
          const msgs = [
            'User process inside container issues an intercepted request...',
            'podbox-guest negotiates capabilities across the UNIX domain socket...',
            'Proxies filter D-Bus methods, Wayland globals, and host-exec requests...',
            'Host session bus, GPU device (/dev/dri) & Wayland compositor handle payload.',
          ];
          setActionLog(l => [msgs[next], ...l.slice(0, 5)]);
        }
        return next;
      });
    }, 2200);
  }
  return () => clearInterval(timer);
}, [isSimulating, activeTab]);

const handleTabChange = (tab: TabType) => {
  setActiveTab(tab);
  setIsSimulating(false);
  if (tab === 'build') setSelectedNode('build_codegen');
  else if (tab === 'runtime') setSelectedNode('rt_guest');
  else setSelectedNode('proto_handshake');
  setActionLog([`Switched to ${tab.toUpperCase()} architecture view.`]);
};

const triggerRuntimeAction = (actionType: RuntimeAction) => {
  setActivePacket(actionType);
  if (actionType === 'notification') {
    setActionLog(prev => [
      '[EVENT] notify-send "Build completed" inside container',
      '-> Intercepted by podbox-guest daemon',
      '-> Forwarded across UNIX socket to D-Bus proxy',
      '-> xdg-dbus-proxy validates --talk=org.freedesktop.Notifications',
      '-> Dispatched to host notification daemon (mako/dunst)',
      ...prev.slice(0, 5),
    ]);
    setSelectedNode('rt_dbus');
  } else if (actionType === 'clipboard') {
    setActionLog(prev => [
      '[EVENT] wl-copy requested inside container',
      '-> Intercepted by podbox-guest clipboard helper',
      '-> Routed through socket host with capability token',
      '-> Synchronized to host Wayland clipboard selection',
      ...prev.slice(0, 5),
    ]);
    setSelectedNode('rt_socket');
  } else if (actionType === 'url') {
    setActionLog(prev => [
      '[EVENT] xdg-open "https://github.com/bethropolis/podbox"',
      '-> Interceptor catches invocation',
      '-> Handed to host-exec broker',
      '-> Host browser opens link outside container sandbox',
      ...prev.slice(0, 5),
    ]);
    setSelectedNode('rt_host');
  } else if (actionType === 'exec') {
    setActionLog(prev => [
      '[EVENT] podbox-host-exec "podman ps"',
      '-> Validates command whitelist in podbox.toml',
      '-> Socket host spawns target binary in host user session',
      ...prev.slice(0, 5),
    ]);
    setSelectedNode('rt_guest');
  }

  setTimeout(() => {
    setActivePacket(null);
  }, 2500);
};

  return { activeTab, selectedNode, isSimulating, simStep, activePacket, actionLog,
    handleTabChange, triggerRuntimeAction, setSelectedNode, setIsSimulating, setSimStep, setActionLog };
}
