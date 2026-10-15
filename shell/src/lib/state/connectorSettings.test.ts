import { describe, expect, it } from 'vitest';
import { GENERIC_BASE, GENERIC_DISTANCE, LD19 } from '../../test/fakeShell';
import { defaultValues, readSettings, settingFields } from './connectorSettings';

describe('connector settings', () => {
  it('come from what the core says about the connector', () => {
    expect(settingFields(GENERIC_BASE).map((f) => f.key)).toEqual([
      'length',
      'width',
      'holonomic',
      'max-vx',
      'max-vy',
      'max-omega',
      'max-accel',
      'rate-hz',
      'built-from',
    ]);
    expect(settingFields(LD19)).toEqual([]);
    expect(settingFields(null)).toEqual([]);
  });

  it("start at the suggested values (SENNEP's footprint), else the defaults", () => {
    expect(defaultValues(settingFields(GENERIC_BASE))).toEqual({
      length: '0.8',
      width: '0.65',
      holonomic: true,
      'max-vx': '0.6',
      'max-vy': '0.6',
      'max-omega': '1.5',
      'max-accel': '1',
      'rate-hz': '20',
      'built-from': [],
    });
  });

  it('reads the form into numbers, booleans and choices, as parts.toml has them', () => {
    const fields = settingFields(GENERIC_DISTANCE);
    const r = readSettings(fields, { ...defaultValues(fields), 'max-m': ' 2.5 ', kind: 'radar' });
    expect(r).toEqual({
      ok: true,
      settings: { 'min-m': 0.02, 'max-m': 2.5, 'fov-rad': 0.26, kind: 'radar' },
    });
  });

  it('writes advanced settings only when they were changed', () => {
    const fields = settingFields(GENERIC_BASE);
    const values = defaultValues(fields);
    const plain = readSettings(fields, values);
    expect(plain.ok && Object.keys(plain.settings)).not.toContain('rate-hz');
    expect(plain.ok && Object.keys(plain.settings)).not.toContain('built-from');
    const changed = readSettings(fields, {
      ...values,
      'rate-hz': '50',
      'built-from': ['left', 'right'],
    });
    expect(changed).toMatchObject({
      ok: true,
      settings: { 'rate-hz': 50, 'built-from': ['left', 'right'] },
    });
  });

  it('says which field is wrong and why, from its limits', () => {
    const fields = settingFields(GENERIC_BASE);
    expect(readSettings(fields, { ...defaultValues(fields), width: 'wide' })).toEqual({
      ok: false,
      key: 'width',
      message: 'Width must be a number.',
    });
    expect(readSettings(fields, { ...defaultValues(fields), 'max-vx': '0' })).toMatchObject({
      key: 'max-vx',
      message: 'Top speed forward must be more than 0.',
    });
    const d = settingFields(GENERIC_DISTANCE);
    expect(readSettings(d, { ...defaultValues(d), 'min-m': '-1' })).toMatchObject({
      message: "Closest it can measure can't be below 0.",
    });
    expect(readSettings(d, { ...defaultValues(d), 'min-m': '0' }).ok).toBe(true);
    const capped = [{ ...d[0]!, max: 1 }];
    expect(readSettings(capped, { 'min-m': '2' })).toMatchObject({
      message: "Closest it can measure can't be above 1.",
    });
  });
});
