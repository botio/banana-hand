import { load, type Store } from "@tauri-apps/plugin-store";
import { invoke } from "@tauri-apps/api/core";

import {
  SETTINGS_SCHEMA_VERSION,
  type Settings,
  type ShortcutRecord,
} from "./domain";

const SETTINGS_KEY = "settings";
const STORE_FILE = "settings.json";

export class SettingsMigrationError extends Error {}

export class SettingsRepository {
  private constructor(private readonly store: Store) {}

  static async open(): Promise<SettingsRepository> {
    return new SettingsRepository(await load(STORE_FILE, { autoSave: false }));
  }

  async read(): Promise<Settings> {
    const saved = await this.store.get<Settings>(SETTINGS_KEY);
    if (!saved) {
      return { schemaVersion: SETTINGS_SCHEMA_VERSION, shortcuts: [] };
    }
    if (saved.schemaVersion > SETTINGS_SCHEMA_VERSION) {
      throw new SettingsMigrationError(
        "設定檔比此 App 新，原檔未被修改。請使用較新的版本，或從備份復原。",
      );
    }
    if (saved.schemaVersion < SETTINGS_SCHEMA_VERSION) {
      await invoke("backup_settings_before_migration");
      await this.store.set(SETTINGS_KEY, this.migrate(saved));
      await this.store.save();
    }
    return this.validate(await this.store.get<Settings>(SETTINGS_KEY));
  }

  async replaceShortcuts(shortcuts: ShortcutRecord[]): Promise<Settings> {
    const settings = { schemaVersion: SETTINGS_SCHEMA_VERSION, shortcuts };
    await this.store.set(SETTINGS_KEY, settings);
    await this.store.save();
    return settings;
  }

  private migrate(settings: Settings): Settings {
    // Version 1 is the initial persisted shape. Future migrations must be
    // forward-only and create their backup through the desktop command first.
    if (settings.schemaVersion === 0) {
      return { schemaVersion: 1, shortcuts: settings.shortcuts ?? [] };
    }
    throw new SettingsMigrationError("設定檔無法安全遷移，原檔未被修改。");
  }

  private validate(settings: Settings | undefined): Settings {
    if (!settings || !Array.isArray(settings.shortcuts)) {
      throw new SettingsMigrationError("設定檔格式無效，原檔未被修改。");
    }
    return {
      schemaVersion: SETTINGS_SCHEMA_VERSION,
      shortcuts: settings.shortcuts
        .filter((shortcut) => shortcut.id && shortcut.name.trim() && shortcut.chord.trim())
        .sort((left, right) => left.order - right.order),
    };
  }
}
