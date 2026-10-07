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

export interface ServiceItem {
  name: string;
  command: string;
  restart: string;
}

/**
 * One `[security].secrets` entry.
 *
 * `secrets = ["name"]` is shorthand for an env secret targeting the same name;
 * anything beyond that needs the detailed form, which is what `target`,
 * `mode` and `source` select.
 */
export interface SecretItem {
  name: string;
  /** `env` → Secret=…,type=env · `mount` → a file in the container. */
  secretType: 'env' | 'mount';
  /** Destination name inside the container; defaults to `name`. */
  target: string;
  /** File mode for mounts, e.g. `0400`. */
  mode: string;
  /** `podman` reads `podman secret`; `systemd` reads a credential from systemd. */
  source: 'podman' | 'systemd';
}

/** True when the entry is exactly what `secrets = ["name"]` expands to. */
export const isBareSecret = (s: SecretItem): boolean =>
  s.secretType === 'env' && !s.target.trim() && !s.mode.trim() && s.source === 'podman';
