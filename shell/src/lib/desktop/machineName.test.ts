import { describe, expect, it } from 'vitest';
import { machineNameProblem, pageTitle, tidyMachineName } from './machineName';

describe('machine name', () => {
  it('accepts host names and explains everything else', () => {
    for (const ok of ['sennep', 'rover-2', 'a', 'x'.repeat(63)])
      expect(machineNameProblem(ok)).toBeNull();
    expect(machineNameProblem('')).toMatch(/name/);
    expect(machineNameProblem('x'.repeat(64))).toMatch(/63/);
    expect(machineNameProblem('Sennep')).toMatch(/lowercase/);
    expect(machineNameProblem('æble')).toMatch(/lowercase/);
    expect(machineNameProblem('-a')).toMatch(/dash/);
    expect(machineNameProblem('a-')).toMatch(/dash/);
  });

  it('tidies what people type', () => {
    expect(tidyMachineName('  My Robot ')).toBe('my-robot');
    expect(tidyMachineName('SENNEP_V2')).toBe('sennep-v2');
    expect(tidyMachineName('a  -  b')).toBe('a-b');
  });

  it('puts the name in the browser tab once it has one', () => {
    expect(pageTitle(undefined)).toBe('Faber');
    expect(pageTitle('faber')).toBe('Faber');
    expect(pageTitle('sennep')).toBe('sennep · Faber');
  });
});
