import { t } from '../i18n';
import { extension } from './filePaths';

/**
 * Connector packages (`.faberpkg`, docs/packages.md): which files are packages, and the words the
 * install dialog and Settings use for what a package is and what it can touch.
 */
export const PACKAGE_EXTENSION = 'faberpkg';

export function isPackageFile(name: string): boolean {
  return extension(name) === PACKAGE_EXTENSION;
}

/** The one port a connector needs, as the API describes it (`PortRequirementInfo`). */
export type PortNeed = { kind: 'serial'; baud: number } | { kind: 'bridge' };

/** What a connector can touch: the port a user picks for each part, and nothing else. */
export function touchWords(port: PortNeed): string {
  return port.kind === 'serial'
    ? t('install.touch.serial', { baud: port.baud })
    : t('install.touch.bridge');
}

/** How it runs, in words: `wasm` or `declarative`. */
export function kindWords(kind: string): string {
  return kind === 'wasm' ? t('install.kind.wasm') : t('install.kind.declarative');
}

/** `"a"` or `"a", "b"`: part names as a sentence fragment. */
export function partList(parts: readonly string[]): string {
  return parts.map((p) => `"${p}"`).join(', ');
}
