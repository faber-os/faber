/** One entry of a right-click menu (menu.ts builds them, CircuitMenu shows them). */
import type { ConnectOption } from '../../lib/circuit/connect';

export interface MenuItem {
  label: string;
  /** A second line, or why the item can't be used. */
  hint?: string;
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  /** A colour chip in front of the label. */
  swatch?: string;
  /** ok / warn marks for pins. */
  mark?: 'ok' | 'warn';
  action?: () => void;
  children?: MenuItem[];
  /** Wires drawn dashed on the canvas while the item is hovered. */
  preview?: ConnectOption;
  separator?: boolean;
}
