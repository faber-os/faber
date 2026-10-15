/**
 * Display units (D43, Settings → Units). The one place where the shell turns SI values into what
 * the user asked to see. Values everywhere else, in the API, recordings and apps, stay metres and
 * radians (docs/api.md, "Conventions"); only the text a person reads changes.
 */

export type LengthUnit = 'metric' | 'imperial';

const METRES_PER_INCH = 0.0254;
const METRES_PER_FOOT = 0.3048;

/** Round steps for scales and rings: 1-2-5 × 10ⁿ metres, or whole inches and feet. */
const IMPERIAL_STEPS_M = [
  ...[1, 2, 3, 6].map((i) => i * METRES_PER_INCH),
  ...[1, 2, 5].map((f) => f * METRES_PER_FOOT),
];

/** "50 cm", "2.5 m"; "6 in", "1.5 ft". */
export function formatLength(m: number, unit: LengthUnit): string {
  if (unit === 'imperial') {
    const feet = m / METRES_PER_FOOT;
    if (Math.abs(feet) < 1 - 1e-9) return `${trim(m / METRES_PER_INCH, 1)} in`;
    return `${trim(feet, 2)} ft`;
  }
  if (Math.abs(m) < 1) return `${trim(m * 100, 1)} cm`;
  return `${trim(m, 3)} m`;
}

/** The smallest round step (m) in `unit` that is at least `minMetres` long. */
export function niceLengthStep(minMetres: number, unit: LengthUnit): number {
  if (unit === 'imperial') {
    const small = IMPERIAL_STEPS_M.find((s) => s >= minMetres - 1e-12);
    if (small !== undefined) return small;
    return metricLike(minMetres / METRES_PER_FOOT) * METRES_PER_FOOT;
  }
  return metricLike(minMetres);
}

/** The smallest 1-2-5 × 10ⁿ at least `min`. */
function metricLike(min: number): number {
  const exp = Math.floor(Math.log10(min));
  for (let e = exp; e <= exp + 1; e++) {
    for (const m of [1, 2, 5]) {
      const step = roundNice(m * 10 ** e);
      if (step >= min - 1e-12) return step;
    }
  }
  return roundNice(10 ** (exp + 1));
}

function trim(v: number, decimals: number): string {
  return String(Number(v.toFixed(decimals)));
}

/** Remove floating-point noise such as 0.30000000000000004. */
function roundNice(v: number): number {
  return Number(v.toPrecision(12));
}

/** "0.3 m/s"; "1 ft/s". */
export function formatSpeed(mps: number, unit: LengthUnit): string {
  if (unit === 'imperial') return `${trim(mps / METRES_PER_FOOT, 2)} ft/s`;
  return `${trim(mps, 2)} m/s`;
}

/** "1.5 rad/s". The units setting has no angle choice yet, so turn rates stay SI. */
export function formatTurnRate(radS: number): string {
  return `${trim(radS, 2)} rad/s`;
}

/** "11.84 V". Electrical units have no setting: they are the same everywhere. */
export function formatVolts(v: number): string {
  return `${trim(v, 2)} V`;
}

/** "1.5 A". */
export function formatAmps(a: number): string {
  return `${trim(a, 2)} A`;
}

/** "31.5 °C". Temperatures stay Celsius, as the capabilities give them. */
export function formatCelsius(c: number): string {
  return `${trim(c, 1)} °C`;
}
