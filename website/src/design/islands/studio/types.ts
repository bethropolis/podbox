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
  /** Optional kill timeout in seconds; `undefined` = none. */
  timeout?: number;
}

export interface ServiceItem {
  name: string;
  command: string;
  restart: string;
}

/** One `[[provision]]` step. */
export interface ProvisionItem {
  name: string;
  run: string;
  /** Duration: `30s`, `5m`, `1h`. Default `5m` (served by the schema data). */
  timeout: string;
  /** `warn` keeps going; `abort` stops the run. */
  onFailure: 'warn' | 'abort';
  /** Extra env, applied on top of the PODBOX_* set. */
  envList: EnvVarItem[];
  /** Run as root inside the container. */
  root: boolean;
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
