/**
 * Accessibility settings (D40): reduce motion, higher contrast and larger text. They are
 * machine-wide; `system` follows what this browser or screen asks for (`prefers-reduced-motion`,
 * `prefers-contrast: more`).
 */

export type Preference = 'system' | 'on' | 'off';
export type TextSize = '100' | '115' | '130' | '150';

export const TEXT_SIZES: readonly TextSize[] = ['100', '115', '130', '150'];

/** Whether the preference is in effect, given what the screen asks for. */
export function prefers(pref: Preference, systemSays: boolean): boolean {
  return pref === 'system' ? systemSays : pref === 'on';
}

/** How much bigger text is: 1 for 100 %, 1.3 for 130 %. Unknown sizes are normal. */
export function textScale(size: TextSize): number {
  return TEXT_SIZES.includes(size) ? Number(size) / 100 : 1;
}
