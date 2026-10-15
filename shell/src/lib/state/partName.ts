import { t } from '../i18n';

export const MAX_PART_NAME = 32;

/**
 * The same rule as the core's `PartName::parse` (core/src/names.rs), so the form can say what is
 * wrong before sending. The core still checks; its answer wins if the two ever disagree.
 */
export function validatePartName(name: string): { message: string; hint: string } | null {
  if (name === '') return { message: t('addPart.name.empty'), hint: t('addPart.name.hint') };
  if (name.length > MAX_PART_NAME) {
    return {
      message: t('addPart.name.tooLong', { name, max: MAX_PART_NAME }),
      hint: t('addPart.name.hint'),
    };
  }
  if (!/^[a-z][a-z0-9_]*$/.test(name)) {
    return { message: t('addPart.name.invalid', { name }), hint: t('addPart.name.hint') };
  }
  return null;
}
