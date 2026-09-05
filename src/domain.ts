export const SETTINGS_SCHEMA_VERSION = 1;

export type BrowserKind = "chrome" | "firefox";

export interface TabTarget {
  browser: BrowserKind;
  browser_instance_id: string;
  session_nonce: string;
  window_id: number;
  tab_id: number;
  generation: number;
}

export interface TabMetadata {
  target: TabTarget;
  title: string;
  url?: string;
}

export interface ShortcutRecord {
  id: string;
  name: string;
  chord: string;
  order: number;
}

export interface Settings {
  schemaVersion: number;
  shortcuts: ShortcutRecord[];
}

export interface RuntimeSnapshot {
  tabs: TabMetadata[];
  cooldown_remaining_seconds: number;
  connected_hosts: number;
  last_bridge_rejection: string | null;
}

export type DispatchOutcome =
  | { rejected: { reason: string } }
  | { partial: { attempts: unknown[] } }
  | { attempted: { attempts: unknown[] } };

export interface NativeHostRegistrationResult {
  manifestPath: string;
  registryLocation: string;
  hostPath: string;
  hostExists: boolean;
}
