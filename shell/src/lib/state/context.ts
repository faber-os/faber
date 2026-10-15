import { getContext, setContext } from 'svelte';
import { connectToCore } from '../api';
import { CoreState } from './core.svelte';

const KEY = Symbol('faber-core');

/** The live core state for the page that is running, talking to the core that served it. */
export function createCore(location: Location = window.location): CoreState {
  return new CoreState(connectToCore(location));
}

export function provideCore(core: CoreState): void {
  setContext(KEY, core);
}

export function useCore(): CoreState {
  const core = getContext<CoreState | undefined>(KEY);
  if (!core) throw new Error('useCore() needs provideCore() in a parent component.');
  return core;
}

/** For tests that render a component on its own. */
export const CORE_CONTEXT_KEY = KEY;
