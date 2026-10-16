import type { Settings } from '../../lib/state/types';

/** What every Settings section gets from the window. */
export interface SectionProps {
  /** The window's id, so ids and radio names stay unique when Settings is open twice. */
  winId: string;
  settings: Settings;
  /** A change is being saved; controls wait. */
  saving: boolean;
  /** Saves a change made to the current settings. */
  change: (patch: (s: Settings) => Settings) => void;
  /** Why this section's controls are disabled for the current role ("Needs the admin role"). */
  lock?: string | null;
}
