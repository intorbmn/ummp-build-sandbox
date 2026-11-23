import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { CoreEvent } from './types';

export const WINDOW_EVENT = 'ummp://event';

/** Calls `handler` for every job event until the returned function is called. */
export function onCoreEvent(handler: (e: CoreEvent) => void): Promise<UnlistenFn> {
  return listen<CoreEvent>(WINDOW_EVENT, (e) => handler(e.payload));
}
