export interface MountItem {
  host: string;
  guest: string;
  mode: string;
}

export interface EnvVarItem {
  key: string;
  value: string;
}

export interface HostExecItem {
  alias: string;
  path: string;
}
