import type { ConnectorInfo, PortInfo } from './types';

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
    return { connector: s.connector, text: `Often used for: ${name}`, reason: s.reason };
  });
}

export function portAvailability(port: PortInfo): { free: boolean; text: string } {
  const by = port.claimedBy;
  // `== null`: a missing field may arrive as nil from MessagePack, which decodes to null.
  if (by == null) return { free: true, text: 'Free' };
  if (by.startsWith('tool:'))
    return { free: false, text: `Used by the ${by.slice(5).replace(/-/g, ' ')}` };
  return { free: false, text: `Used by the part "${by}"` };
}
