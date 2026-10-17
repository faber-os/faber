/**
 * A part someone adds: its size and one line per pin. The same facts a contributor writes in a
 * part file's `[drawing]` table (docs/wiring-diagram.md §2), which `drawingToml` prints.
 */
import { edgePins } from './parts';
import type { PartDef, PinDef } from './types';

const MM = 1.27;
type Edge = 'top' | 'right' | 'bottom' | 'left';
const EDGES: Edge[] = ['top', 'right', 'bottom', 'left'];

export interface NewPart {
  name: string;
  widthMm: number;
  heightMm: number;
  color: string;
  pins: string;
  art: string | null;
}

export function parsePins(text: string): { list: [string, Edge, Omit<PinDef, 'id' | 'label' | 'x' | 'y' | 'd'>][]; errors: string[] } {
  const errors: string[] = [];
  const list: [string, Edge, Omit<PinDef, 'id' | 'label' | 'x' | 'y' | 'd'>][] = [];
  text.split('\n').forEach((line, i) => {
    const t = line.trim();
    if (!t || t.startsWith('#')) return;
    const words = t.split(/\s+/);
    const [label, edge, kind] = words as [string, string | undefined, string | undefined];
    if (!EDGES.includes(edge as Edge)) {
      errors.push(`Line ${i + 1}: the edge must be top, bottom, left or right.`);
      return;
    }
    const extra = words.slice(3).filter((w) => w !== 'optional');
    const optional = words.includes('optional');
    const drives = extra.find((w) => w.startsWith('drives='));
    const num = extra.find((w) => /^[\d.]+(-[\d.]+)?$/.test(w));
    const base = { optional, ...(drives ? { drives: parseFloat(drives.slice(7)), out: true } : {}) };
    let props: Omit<PinDef, 'id' | 'label' | 'x' | 'y' | 'd'>;
    switch (kind) {
      case 'vcc': {
        const [a, b] = (num ?? '3.3-5.5').split('-').map(parseFloat) as [number, number | undefined];
        props = { ...base, kind: 'vcc', min: a, max: b ?? a };
        break;
      }
      case 'gnd':
        props = { ...base, kind: 'gnd' };
        break;
      case 'io':
      case 'pwm':
      case 'analog':
      case 'sda':
      case 'scl':
        props = { ...base, kind: 'sig', needs: [kind] };
        break;
      case 'supply':
        props = { ...base, kind: 'supply', volts: parseFloat(num ?? '5'), terminal: true };
        break;
      case 'motor':
        props = { ...base, kind: 'motor-in' };
        break;
      default:
        errors.push(`Line ${i + 1}: unknown kind "${kind ?? ''}".`);
        return;
    }
    list.push([label, edge as Edge, props]);
  });
  const ids = list.map((p) => p[0]);
  if (new Set(ids).size !== ids.length) errors.push('Two pins have the same name.');
  return { list, errors };
}

export function makeDef(input: NewPart): { def: PartDef; errors: string[] } {
  const { list, errors } = parsePins(input.pins);
  const count: Record<Edge, number> = { top: 0, right: 0, bottom: 0, left: 0 };
  for (const p of list) count[p[1]] += 1;
  let w = Math.max(6, Math.round((input.widthMm || 20) / MM));
  let h = Math.max(6, Math.round((input.heightMm || 10) / MM));
  w = Math.max(w, 2 * Math.max(count.top, count.bottom) + 2);
  h = Math.max(h, 2 * Math.max(count.left, count.right) + 2);
  w += w & 1;
  h += h & 1;
  const name = input.name.trim() || 'New part';
  const id = 'custom-' + name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  return { def: { id, name, covers: 'Your part', kind: 'part', look: 'generic', color: input.color, art: input.art, w, h, pins: edgePins(list, w, h) }, errors };
}

export function drawingToml(d: PartDef): string {
  const lines = d.pins.map((p) => {
    const edge = EDGES[p.d]!;
    const at = (edge === 'top' || edge === 'bottom' ? p.x : p.y) * MM;
    return `  { id = "${p.label}", edge = "${edge}", at-mm = ${at.toFixed(2)} },`;
  });
  const look = d.art ? 'art = "top-view.svg"   # shipped beside the part file' : `colour = "${d.color}"`;
  return `[drawing]\nsize-mm = [${(d.w * MM).toFixed(1)}, ${(d.h * MM).toFixed(1)}]\n${look}\npins = [\n${lines.join('\n')}\n]`;
}
