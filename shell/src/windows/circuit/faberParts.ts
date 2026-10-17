/** Faber's parts as the circuit lab reads them (lib/circuit/fromParts.ts). */
import type { FaberPart } from '../../lib/circuit/fromParts';
import type { CoreState } from '../../lib/state/core.svelte';
import type { Result } from '../../lib/state/result';
import { t } from '../../lib/i18n';

export interface FaberMachine {
  parts: FaberPart[];
  /** What the machine is: a Pi (real buses) or a computer with USB ports. */
  host: 'pi' | 'computer';
}

export async function faberParts(core: CoreState): Promise<Result<FaberMachine>> {
  const list = core.parts;
  if (!list)
    return {
      ok: false,
      error: {
        code: 'not-connected',
        message: t('circuit.window.noCore'),
        hint: t('circuit.window.noCoreHint'),
      },
    };
  return {
    ok: true,
    value: {
      host: core.system?.buses === 'system' ? 'pi' : 'computer',
      parts: list.map((p) => ({
        name: p.name,
        connector: p.connector,
        port: p.port,
        address: p.address,
        bridge: p.bridge,
        pins: p.pins?.map((x) => ({ role: x.role, label: x.label })),
        capabilities: p.signals.map((s) => String(s.capability)),
        simulated: !!p.simulate,
      })),
    },
  };
}
