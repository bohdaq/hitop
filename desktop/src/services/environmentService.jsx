/**
 * Environment Service
 *
 * Environments live in hitop-core's environments.json (shared with the CLI's
 * `hitop env`): { active: id | null, environments: [{ id, name, variables }] }.
 */
import { invoke } from '@tauri-apps/api/core';

export const loadEnvironments = async () => {
  try {
    const data = await invoke('load_environments');
    return { active: data?.active ?? null, environments: data?.environments ?? [] };
  } catch (error) {
    console.error('Error loading environments:', error);
    return { active: null, environments: [] };
  }
};

export const saveEnvironments = async (environments, active) => {
  try {
    await invoke('save_environments', { environments: { active: active ?? null, environments } });
  } catch (error) {
    console.error('Error saving environments:', error);
  }
};

export default { loadEnvironments, saveEnvironments };
