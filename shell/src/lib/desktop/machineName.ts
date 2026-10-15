import { t } from '../i18n';

/**
 * The machine's name (Settings → This machine). Same rule as the core (`valid_machine_name` in
 * core/src/settings.rs): it becomes the network name `<name>.local` on the Pi image, so it must be
 * a valid host name.
 */
export function machineNameProblem(name: string): string | null {
  if (name.length === 0) return t('settings.machine.problem.empty');
  if (name.length > 63) return t('settings.machine.problem.tooLong');
  if (!/^[a-z0-9-]+$/.test(name)) return t('settings.machine.problem.characters');
  if (name.startsWith('-') || name.endsWith('-')) return t('settings.machine.problem.dash');
  return null;
}

/** What someone probably meant: "My Robot" → "my-robot". Leaves anything it can't fix. */
export function tidyMachineName(typed: string): string {
  return typed
    .trim()
    .toLowerCase()
    .replace(/[\s_]+/g, '-')
    .replace(/-+/g, '-');
}

/** The browser tab's title: just "Faber" until the machine has its own name. */
export function pageTitle(name: string | undefined): string {
  return !name || name === 'faber' ? 'Faber' : `${name} · Faber`;
}
