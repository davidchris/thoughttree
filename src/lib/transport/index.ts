import type { BackendTransport } from './types';
import { TauriTransport } from './TauriTransport';

let backendTransport: BackendTransport | null = null;

export function getBackendTransport(): BackendTransport {
  if (!backendTransport) {
    backendTransport = new TauriTransport();
  }
  return backendTransport;
}

export function setBackendTransport(transport: BackendTransport): void {
  backendTransport = transport;
}

export type * from './types';
export { KagiImportError, StaleRevisionError } from './types';
export { TauriTransport } from './TauriTransport';
