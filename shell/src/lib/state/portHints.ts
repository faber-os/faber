import { t } from '../i18n';
import { BRIDGE_CONNECTOR } from './bridges';
import type { ConnectorInfo, PortInfo, PortProblem } from './types';

export interface PortHint {
  connector: string;
  /** Short, for the port list: "Often used for: LD19 LiDAR". */
  text: string;
  /** The core's reason, in its own words. */
  reason: string;
}

export function portHints(port: PortInfo, connectors: ConnectorInfo[]): PortHint[] {
  return port.suggestions.map((s) => {
    const name = connectors.find((c) => c.id === s.connector)?.name ?? s.connector;
    return {
      connector: s.connector,
      // Only a board that answered is suggested as one, so this one can say so plainly.
      text:
        s.connector === BRIDGE_CONNECTOR
          ? t('addPart.port.faberlinkBoard')
          : t('addPart.port.oftenUsedFor', { name }),
      reason: s.reason,
    };
  });
}

export function portAvailability(port: PortInfo): { free: boolean; text: string } {
  const by = port.claimedBy;
  // `== null`: a missing field may arrive as nil from MessagePack, which decodes to null.
  if (by == null) return { free: true, text: t('addPart.port.free') };
  if (by.startsWith('tool:'))
    return {
      free: false,
      text: t('addPart.port.usedByTool', { tool: by.slice(5).replace(/-/g, ' ') }),
    };
  return { free: false, text: t('addPart.port.usedByPart', { part: by }) };
}

/**
 * What kind of link a port is, in plain words, so a Bluetooth link is never mistaken for a
 * cable just because the system gave it a COM number.
 */
export function portKindLabel(port: Pick<PortInfo, 'kind'>): string {
  switch (port.kind) {
    case 'usb-serial':
      return t('parts.portKind.usbSerial');
    case 'serial':
      return t('parts.portKind.serial');
    case 'bluetooth':
      return t('parts.portKind.bluetooth');
    case 'replay':
      return t('parts.portKind.replay');
    default:
      return port.kind;
  }
}

/** Port problems with the same code shown once, with how many there are. */
export function groupPortProblems(
  problems: readonly PortProblem[],
): (PortProblem & { count: number })[] {
  const out = new Map<string, PortProblem & { count: number }>();
  for (const p of problems) {
    const seen = out.get(p.code);
    if (seen) seen.count++;
    else out.set(p.code, { ...p, count: 1 });
  }
  return [...out.values()];
}
