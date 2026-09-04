import type { ImportedConversation, ImportedConversationTurn } from './import';
import type { TurnProvenance, UrlTurnReference } from './types';

export const KAGI_EXPORT_MAX_BYTES = 16 * 1024 * 1024;

type KagiExportErrorCode =
  | 'input_too_large'
  | 'invalid_json'
  | 'unsupported_version'
  | 'no_messages';

interface InputTooLargeDetail {
  inputBytes: number;
  limitBytes: number;
}

export class KagiExportError extends Error {
  readonly code: KagiExportErrorCode;
  readonly foundVersion?: unknown;
  readonly inputBytes?: number;
  readonly limitBytes?: number;

  constructor(code: 'input_too_large', detail: InputTooLargeDetail);
  constructor(code: 'unsupported_version', foundVersion: unknown);
  constructor(code: 'invalid_json' | 'no_messages');
  constructor(code: KagiExportErrorCode, detail?: unknown) {
    const message =
      code === 'input_too_large'
        ? `Kagi export exceeds the ${(detail as InputTooLargeDetail).limitBytes}-byte input limit (${(detail as InputTooLargeDetail).inputBytes} bytes)`
        : code === 'unsupported_version'
          ? `Unsupported Kagi export version: ${String(detail)}`
          : code === 'no_messages'
            ? 'Kagi export contains no messages'
            : 'Invalid Kagi export JSON';
    super(message);
    this.name = 'KagiExportError';
    this.code = code;
    if (code === 'unsupported_version') this.foundVersion = detail;
    if (code === 'input_too_large') {
      this.inputBytes = (detail as InputTooLargeDetail).inputBytes;
      this.limitBytes = (detail as InputTooLargeDetail).limitBytes;
    }
  }
}

interface KagiMessage {
  role?: unknown;
  content?: unknown;
  model_name?: unknown;
  references?: unknown;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function asMessage(value: unknown): KagiMessage | undefined {
  return isRecord(value) ? value : undefined;
}

function text(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

/** True only for parseable http/https URLs. Never dereferences the URL. */
export function isWebUrl(value: string): boolean {
  try {
    const protocol = new URL(value).protocol;
    return protocol === 'http:' || protocol === 'https:';
  } catch {
    return false;
  }
}

function provenance(message: KagiMessage): TurnProvenance {
  const content = text(message.content);
  const citedIndexes = new Set(
    Array.from(content.matchAll(/【(\d+)】/gu), (match) => Number(match[1]))
  );
  const references: UrlTurnReference[] = [];

  if (Array.isArray(message.references)) {
    for (const value of message.references) {
      if (!isRecord(value) || typeof value.url !== 'string' || !isWebUrl(value.url)) continue;
      const reference: UrlTurnReference = {
        type: 'url',
        url: value.url,
        relations: [
          typeof value.index === 'number' && citedIndexes.has(value.index)
            ? 'cited'
            : 'consulted',
        ],
      };
      if (typeof value.title === 'string') reference.title = value.title;
      if (typeof value.domain === 'string') reference.domain = value.domain;
      if (typeof value.index === 'number' && Number.isFinite(value.index)) {
        reference.index = value.index;
      }
      if (typeof value.percentage === 'number' && Number.isFinite(value.percentage)) {
        reference.percentage = value.percentage;
      }
      if (typeof value.is_search_result === 'boolean') {
        reference.is_search_result = value.is_search_result;
      }
      references.push(reference);
    }
  }

  return { completeness: 'complete', references, activity: [] };
}

function turnFromUser(user: KagiMessage, assistant?: KagiMessage): ImportedConversationTurn {
  if (!assistant) {
    return {
      userMessage: text(user.content),
      assistantAnswer: '',
      incomplete: true,
    };
  }

  const turn: ImportedConversationTurn = {
    userMessage: text(user.content),
    assistantAnswer: text(assistant.content),
    provenance: provenance(assistant),
  };
  if (typeof assistant.model_name === 'string') turn.model = assistant.model_name;
  return turn;
}

function tooLarge(inputBytes: number, limitBytes: number): KagiExportError {
  return new KagiExportError('input_too_large', { inputBytes, limitBytes });
}

/**
 * Enforces the byte cap before materializing untrusted input: strings are
 * bounded by their code-unit length (UTF-8 is never shorter) before encoding,
 * and bytes are bounded before decoding. Malformed UTF-8 is rejected rather
 * than silently replaced.
 */
function decodeInput(input: string | Uint8Array, maxBytes: number): string {
  if (typeof input === 'string') {
    if (input.length > maxBytes) throw tooLarge(input.length, maxBytes);
    const bytes = new TextEncoder().encode(input).byteLength;
    if (bytes > maxBytes) throw tooLarge(bytes, maxBytes);
    return input;
  }
  if (input.byteLength > maxBytes) throw tooLarge(input.byteLength, maxBytes);
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(input);
  } catch {
    throw new KagiExportError('invalid_json');
  }
}

export function parseKagiExport(
  input: string | Uint8Array,
  maxBytes = KAGI_EXPORT_MAX_BYTES
): ImportedConversation {
  const source = decodeInput(input, maxBytes);

  let exportData: unknown;
  try {
    exportData = JSON.parse(source);
  } catch {
    throw new KagiExportError('invalid_json');
  }

  if (!isRecord(exportData) || exportData.version !== 1) {
    throw new KagiExportError('unsupported_version', isRecord(exportData) ? exportData.version : undefined);
  }

  const conversation = isRecord(exportData.conversation) ? exportData.conversation : {};
  const messages = Array.isArray(exportData.messages)
    ? exportData.messages
    : Array.isArray(conversation.messages)
      ? conversation.messages
      : [];
  const turns: ImportedConversationTurn[] = [];
  let pendingUser: KagiMessage | undefined;

  for (const value of messages) {
    const message = asMessage(value);
    if (!message) {
      if (pendingUser) {
        turns.push(turnFromUser(pendingUser));
        pendingUser = undefined;
      }
      continue;
    }

    if (message.role === 'user') {
      if (pendingUser) turns.push(turnFromUser(pendingUser));
      pendingUser = message;
    } else if (message.role === 'assistant' && pendingUser) {
      turns.push(turnFromUser(pendingUser, message));
      pendingUser = undefined;
    } else if (message.role !== 'assistant') {
      if (pendingUser) {
        turns.push(turnFromUser(pendingUser));
        pendingUser = undefined;
      }
    }
  }
  if (pendingUser) turns.push(turnFromUser(pendingUser));
  if (turns.length === 0) throw new KagiExportError('no_messages');

  return {
    importKey: text(conversation.title) || 'Kagi conversation',
    turns,
  };
}
