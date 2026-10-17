/** Every part type the lab knows, and how an id (plain or parametric) becomes a part. */
import { LIBRARY, PRESETS, variant } from './library';
import { BUILTIN } from './parts';
import type { DefLookup, PartDef } from './types';

export const CATALOG: PartDef[] = [...BUILTIN, ...LIBRARY];

const cache = new Map<string, PartDef>();

/** Looks parts up by id: built in, from the parts list, made by settings, or added by people. */
export function lookup(custom: PartDef[] = []): DefLookup {
  return (id: string) => {
    const fixed = CATALOG.find((d) => d.id === id) ?? custom.find((d) => d.id === id);
    if (fixed) return fixed;
    let d = cache.get(id);
    if (!d) {
      d = variant(id);
      if (d) cache.set(id, d);
    }
    return d;
  };
}

/** What the parts list shows: every fixed part and the ready-made parametric ones. */
export function listed(custom: PartDef[] = []): PartDef[] {
  const look = lookup(custom);
  const presets = PRESETS.map((p) => ({
    ...look(p.id)!,
    id: p.id,
    name: p.name,
    covers: p.covers,
  }));
  return [...CATALOG, ...presets, ...custom];
}
