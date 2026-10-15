import { prefers, textScale, type Preference, type TextSize } from '../lib/desktop/accessibility';

/**
 * Reduce motion and larger text (Settings → Accessibility, D40). The choices live in the core's
 * settings, shared by every screen; `system` follows this browser's `prefers-reduced-motion`.
 * App.svelte puts the result on the page root. Higher contrast is part of the colours, so the
 * ThemeController handles it.
 */
export class DisplayPrefs {
  motion = $state<Preference>('system');
  systemReduceMotion = $state(false);
  textSize = $state<TextSize>('100');
  reduceMotion = $derived(prefers(this.motion, this.systemReduceMotion));
  /** The page root's font size: every size in the shell is in rem, so this scales all text. */
  fontSize = $derived(`${Math.round(textScale(this.textSize) * 100)}%`);

  constructor(reducedMotion: MediaQueryList | null) {
    if (reducedMotion) {
      this.systemReduceMotion = reducedMotion.matches;
      reducedMotion.addEventListener('change', (e) => (this.systemReduceMotion = e.matches));
    }
  }

  /** Follow the settings (called whenever they arrive or change). */
  set(a: { reduceMotion: Preference; textSize: TextSize; higherContrast?: Preference }): void {
    this.motion = a.reduceMotion;
    this.textSize = a.textSize;
  }
}
