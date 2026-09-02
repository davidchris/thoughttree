import { invoke } from '@tauri-apps/api/core';
import { withoutNullEntries } from '../types';
import type { AgentProvider, ProviderPaths, StoredProviderRecord } from '../types';

export function getNotesDirectory(): Promise<string | null> {
  return invoke<string | null>('get_notes_directory');
}

export async function setNotesDirectory(path: string): Promise<void> {
  await invoke('set_notes_directory', { path });
}

export function pickNotesDirectory(): Promise<string | null> {
  return invoke<string | null>('pick_notes_directory');
}

export function newProjectDialog(): Promise<string | null> {
  return invoke<string | null>('new_project_dialog');
}

export function openProjectDialog(): Promise<string | null> {
  return invoke<string | null>('open_project_dialog');
}

export function pickKagiExport(): Promise<string | null> {
  return invoke<string | null>('pick_kagi_export');
}

export function getRecentProjects(): Promise<string[]> {
  return invoke<string[]>('get_recent_projects');
}

export async function addRecentProject(path: string): Promise<void> {
  await invoke('add_recent_project', { path });
}

export async function removeRecentProject(path: string): Promise<void> {
  await invoke('remove_recent_project', { path });
}

export function exportMarkdown(content: string, defaultName: string): Promise<string | null> {
  return invoke<string | null>('export_markdown', { content, defaultName });
}

export async function getProviderPaths(): Promise<ProviderPaths> {
  return withoutNullEntries(await invoke<StoredProviderRecord>('get_provider_paths'));
}

export async function setProviderPath(
  provider: AgentProvider,
  path: string | null
): Promise<void> {
  await invoke('set_provider_path', { provider, path });
}

export function validateProviderPath(provider: AgentProvider, path: string): Promise<string> {
  return invoke<string>('validate_provider_path', { provider, path });
}

export function pickProviderExecutable(provider: AgentProvider): Promise<string | null> {
  return invoke<string | null>('pick_provider_executable', { provider });
}
