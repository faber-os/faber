import { describe, expect, it } from 'vitest';
import {
  boardNameFor,
  freeName,
  partGroup,
  partNameProblem,
  pickable,
  pinLabel,
  pinLook,
  planOfBridge,
  readSetting,
  roleNameIn,
  templateGroups,
  wireChanges,
} from './guided';
import { SONAR_ON_UNO, TEMPLATES } from '../../test/guidedFixtures';
import type { BoardPlan, PartInfo, PartSetting, PartTemplate, RoleOptions } from './types';

const [trig, echo] = SONAR_ON_UNO.roles as [RoleOptions, RoleOptions];
const pins = { trig: 'D4', echo: 'D7' };
const first = { trig: 'D4', echo: 'D7' };

describe('how a pin looks for the role being filled', () => {
  it('knows chosen, suitable, reserved and this part’s other pin', () => {
    const roles = SONAR_ON_UNO.roles;
    expect(pinLook(trig, 'D4', pins, first, roles).kind).toBe('chosen');
    expect(pinLook(trig, 'D8', pins, first, roles).kind).toBe('suitable');
    expect(pinLook(trig, 'D0', pins, first, roles)).toEqual({
      kind: 'reserved',
      reason: 'D0 and D1 carry the USB link to Faber (Serial).',
    });
    // The core says Echo's pin is in use; to the picker it is this part's other pin (a swap).
    expect(pinLook(trig, 'D7', pins, first, roles)).toEqual({ kind: 'other-role', reason: 'Echo' });
    // Faber's first pick stays marked once another pin is chosen.
    expect(pinLook(trig, 'D4', { ...pins, trig: 'D8' }, first, roles).kind).toBe('suggested');
  });

  it('marks pins in use by other parts, and pins that can’t do the job, with why', () => {
    const role: RoleOptions = {
      ...echo,
      inUse: [{ pin: 'D9', by: 'rear (Trig)' }],
      unsuitable: [{ pin: 'A6', reason: 'Echo needs a digital pin: A6 is analog only.' }],
    };
    expect(pinLook(role, 'D9', {}, {}, [role])).toEqual({ kind: 'in-use', reason: 'rear (Trig)' });
    const a6 = pinLook(role, 'A6', {}, {}, [role]);
    expect(a6.kind).toBe('unsuitable');
    expect(pinLabel('A6', a6, 'Echo')).toBe(
      'A6, unsuitable for Echo: Echo needs a digital pin: A6 is analog only.',
    );
    expect(pinLabel('D8', { kind: 'suitable' }, 'Echo')).toBe('D8, suitable for Echo');
    expect(pinLabel('D9', { kind: 'in-use', reason: 'rear (Trig)' }, 'Echo')).toBe(
      'D9, in use: rear (Trig)',
    );
    expect(pickable('suitable') && pickable('other-role')).toBe(true);
    expect(pickable('in-use') || pickable('reserved') || pickable('unsuitable')).toBe(false);
  });
});

describe('the part catalogue', () => {
  it('groups templates by what they do, any new capability under other sensors', () => {
    const imu: PartTemplate = { ...TEMPLATES[0]!, id: 'imu', name: 'IMU', capability: 'imu' };
    const provider: PartTemplate = { ...TEMPLATES[0]!, id: 'pca', name: 'PCA9685' };
    delete provider.capability;
    const groups = templateGroups([...TEMPLATES, imu, provider]);
    expect(groups.map((g) => [g.group, g.templates.map((x) => x.id)])).toEqual([
      ['distance', ['ultrasonic', 'ir-distance']],
      ['motion', ['motor']],
      ['power', ['battery']],
      ['sensors', ['imu']],
      ['other', ['pca']],
    ]);
    expect(groups[0]!.title).toBe('Distance and obstacles');
    expect(partGroup('button@1')).toBe('inputs');
  });
});

describe('settings', () => {
  const max: PartSetting = TEMPLATES[0]!.settings.find((s) => s.id === 'max-m')!;
  const cells = TEMPLATES.find((x) => x.id === 'battery')!.settings.find((s) => s.id === 'cells')!;

  it('reads numbers within the template’s limits, and says the limits with the unit', () => {
    expect(readSetting(max, '2,5')).toEqual({ ok: true, value: 2.5 });
    expect(readSetting(max, '9')).toEqual({
      ok: false,
      message: 'Longest distance: from 0.1 to 4 m.',
    });
    expect(readSetting(max, 'far')).toEqual({
      ok: false,
      message: 'Longest distance: type a number.',
    });
    expect(readSetting(cells, '3.5').ok).toBe(false);
    expect(readSetting(cells, '4')).toEqual({ ok: true, value: 4 });
  });
});

describe('names', () => {
  const plan: BoardPlan = {
    board: 'uno',
    name: 'uno',
    parts: { front: { template: 'ultrasonic', pins: {}, settings: {} } },
  };
  it('checks a part’s name live, against the board’s other parts', () => {
    expect(partNameProblem('front', plan)).toMatch(/already a part called front/);
    expect(partNameProblem('front', plan, 'front')).toBeNull();
    expect(partNameProblem('Front', plan)).not.toBeNull();
    expect(partNameProblem('rear', plan)).toBeNull();
    expect(freeName('front', ['front', 'front_2'])).toBe('front_3');
    expect(boardNameFor('mega2560')).toBe('mega');
    expect(boardNameFor('nano-old')).toBe('nano');
  });
});

describe('wires that move', () => {
  it('lists new, moved and removed wires with the roles’ names', () => {
    const before: BoardPlan = {
      board: 'uno',
      name: 'uno',
      parts: {
        front: { template: 'ultrasonic', pins: { trig: 'D4', echo: 'D7' }, settings: {} },
        old: { template: 'ir-distance', pins: { signal: 'A0' }, settings: {} },
      },
    };
    const after: BoardPlan = {
      ...before,
      parts: {
        front: { template: 'ultrasonic', pins: { trig: 'D4', echo: 'D8' }, settings: {} },
        bat: { template: 'battery', pins: { sense: 'A1' }, settings: {} },
      },
    };
    const name = (p: Parameters<typeof roleNameIn>[1], r: string) => roleNameIn(TEMPLATES, p, r);
    expect(wireChanges(before, after, name)).toEqual([
      { part: 'bat', role: 'Sense', to: 'A1' },
      { part: 'front', role: 'Echo', from: 'D7', to: 'D8' },
      { part: 'old', role: 'Signal', from: 'A0' },
    ]);
  });
});

describe('the guided board a FaberLink board runs', () => {
  const bridge = {
    name: 'uno',
    connector: 'faber-bridge',
    signals: [],
    state: { kind: 'running' },
    board: { link: 'ready', board: 'uno', hardwareEstop: false, failsafe: false },
  } as unknown as PartInfo;
  it('is the plan of the name the board says, unless its code was taken over', () => {
    const plans = [{ name: 'uno', board: 'uno', parts: 1, takenOver: false }];
    expect(planOfBridge(bridge, plans)).toBe('uno');
    expect(planOfBridge(bridge, [{ ...plans[0]!, takenOver: true }])).toBeNull();
    expect(planOfBridge({ ...bridge, connector: 'ldrobot-ld19' }, plans)).toBeNull();
  });
});
