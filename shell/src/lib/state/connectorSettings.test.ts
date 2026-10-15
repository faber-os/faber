import { describe, expect, it } from 'vitest';
import { defaultValues, readSettings, settingFields } from './connectorSettings';

const BASE = 'generic-planar-base';

describe('generic part settings', () => {
  it("start at SENNEP's footprint and limits", () => {
    const values = defaultValues(settingFields(BASE));
    expect(values).toMatchObject({
      length: '0.8',
      width: '0.65',
      holonomic: true,
      'max-vx': '0.6',
      'max-omega': '1.5',
    });
  });

  it('has no settings form for connectors that are not generic', () => {
    expect(settingFields('ldrobot-ld19')).toEqual([]);
    expect(settingFields(null)).toEqual([]);
  });

  it('every field and choice has a label for people', () => {
    for (const id of ['generic-planar-base', 'generic-motor', 'generic-distance']) {
      for (const f of settingFields(id)) {
        expect(f.label()).not.toMatch(/^addPart\./);
        if (f.kind === 'choice') for (const c of f.choices) expect(c.label()).not.toMatch(/\./);
      }
    }
  });

  it('reads the form into numbers, booleans and choices, as parts.toml has them', () => {
    const fields = settingFields('generic-distance');
    const r = readSettings(fields, { ...defaultValues(fields), 'max-m': ' 2.5 ', kind: 'radar' });
    expect(r).toEqual({
      ok: true,
      settings: { 'min-m': 0.02, 'max-m': 2.5, 'fov-rad': 0.26, kind: 'radar' },
    });
  });

  it('says which field is wrong and why', () => {
    const fields = settingFields(BASE);
    expect(readSettings(fields, { ...defaultValues(fields), width: 'wide' })).toEqual({
      ok: false,
      key: 'width',
      message: 'Width must be a number.',
    });
    expect(readSettings(fields, { ...defaultValues(fields), 'max-vx': '0' })).toMatchObject({
      key: 'max-vx',
      message: 'Top speed forward must be more than 0.',
    });
    const d = settingFields('generic-distance');
    expect(readSettings(d, { ...defaultValues(d), 'min-m': '-1' })).toMatchObject({
      message: "Closest it can measure can't be below 0.",
    });
    expect(readSettings(d, { ...defaultValues(d), 'min-m': '0' }).ok).toBe(true);
  });
});
