export const MAX_PART_NAME = 32;

const HINT = 'Use lowercase letters, digits and _, starting with a letter, e.g. front_lidar.';

/**
 * The same rule as the core's `PartName::parse` (core/src/names.rs), so the form can say what is
 * wrong before sending. The core still checks; its answer wins if the two ever disagree.
 */
export function validatePartName(name: string): { message: string; hint: string } | null {
  if (name === '') return { message: 'The part needs a name.', hint: HINT };
  if (name.length > MAX_PART_NAME) {
    return { message: `"${name}" is longer than ${MAX_PART_NAME} characters.`, hint: HINT };
  }
  if (!/^[a-z][a-z0-9_]*$/.test(name)) {
    return { message: `"${name}" isn't a valid part name.`, hint: HINT };
  }
  return null;
}
