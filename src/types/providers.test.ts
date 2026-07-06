import { describe, expect, it } from 'vitest';
import {
  ALL_PROVIDERS,
  DEFAULT_PROVIDER,
  PROVIDER_DESCRIPTORS,
  PROVIDER_DISPLAY_NAMES,
  PROVIDER_SHORT_NAMES,
  PROVIDER_SUPPORTED_EFFORTS,
  withoutNullEntries,
  type ReasoningEffort,
  type StoredProviderRecord,
} from './index';

describe('provider descriptors', () => {
  it('lists every provider exactly once', () => {
    expect(ALL_PROVIDERS).toEqual(['claude-code', 'gemini-cli', 'codex']);
    expect(new Set(ALL_PROVIDERS).size).toBe(PROVIDER_DESCRIPTORS.length);
  });

  it('names Codex to match the Rust descriptor', () => {
    expect(PROVIDER_DISPLAY_NAMES['codex']).toBe('Codex');
    expect(PROVIDER_SHORT_NAMES['codex']).toBe('Codex');
  });

  it('derives name records from the descriptor table', () => {
    for (const descriptor of PROVIDER_DESCRIPTORS) {
      expect(PROVIDER_DISPLAY_NAMES[descriptor.id]).toBe(descriptor.displayName);
      expect(PROVIDER_SHORT_NAMES[descriptor.id]).toBe(descriptor.shortName);
    }
  });

  it('keeps the default provider in the descriptor table', () => {
    expect(ALL_PROVIDERS).toContain(DEFAULT_PROVIDER);
  });

  it('declares supported reasoning efforts per provider', () => {
    expect(PROVIDER_DESCRIPTORS.find((d) => d.id === 'claude-code')?.supportedEfforts).toEqual([
      'low',
      'medium',
      'high',
      'xhigh',
    ]);
    expect(PROVIDER_DESCRIPTORS.find((d) => d.id === 'codex')?.supportedEfforts).toEqual([
      'low',
      'medium',
      'high',
      'xhigh',
    ]);
    expect(PROVIDER_DESCRIPTORS.find((d) => d.id === 'gemini-cli')?.supportedEfforts).toEqual([]);

    for (const descriptor of PROVIDER_DESCRIPTORS) {
      expect(PROVIDER_SUPPORTED_EFFORTS[descriptor.id]).toBe(descriptor.supportedEfforts);
    }
  });

  it('strips null entries from stored effort records', () => {
    const stored: StoredProviderRecord<ReasoningEffort> = {
      'claude-code': 'high',
      codex: null,
    };

    expect(withoutNullEntries(stored)).toEqual({ 'claude-code': 'high' });
  });
});
