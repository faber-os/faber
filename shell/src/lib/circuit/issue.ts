/** What a check finds (checks.ts, advice.ts): types only, so both can share them. */

export type AdviceCode =
  | 'led-no-resistor'
  | 'led-backwards'
  | 'needs-pull-up'
  | 'needs-pull-down'
  | 'needs-divider'
  | 'needs-flyback';

export type Severity = 'error' | 'warn';

export type IssueCode =
  | 'self-joined'
  | 'short'
  | 'supplies-fight'
  | 'no-power'
  | 'over-voltage'
  | 'under-voltage'
  | 'hot-regulator'
  | 'logic-joined'
  | 'tx-tx'
  | 'board-pins-joined'
  | 'signal-on-power'
  | 'signal-unrouted'
  | 'needs-cap'
  | 'drives-into'
  | 'serial-pin'
  | 'outputs-fight'
  | 'load-on-board'
  | 'shifter-tx-tx'
  | 'not-connected'
  | 'no-common-ground'
  | 'boards-no-ground'
  | 'cable-mismatch'
  | 'over-budget'
  | 'caution'
  | 'no-route'
  | AdviceCode;

export interface Issue {
  sev: Severity;
  code: IssueCode;
  text: string;
  more: string;
  parts: string[];
  wires: string[];
  /** The pins it is about, most important first. */
  keys: string[];
}
