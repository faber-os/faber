import { describe, expect, it } from 'vitest';
import { noticeView } from './notices';
import type { Notice } from './types';

const notice = (level: Notice['level'], volts: number, address = 'pack/battery'): Notice => ({
  kind: 'battery-level',
  tUs: 1,
  address,
  level,
  volts,
});

describe('notices', () => {
  it('say what happened to which battery, and what to do', () => {
    expect(noticeView(notice('low', 10.4))).toEqual({
      message: 'The battery of pack is low (10.4 V).',
      hint: 'Finish what you are doing and charge it soon.',
      tone: 'warn',
      sticky: true,
    });
    const critical = noticeView(notice('critical', 9.812, 'app:monitor/battery'));
    expect(critical.message).toBe('The battery of app:monitor is critical (9.81 V).');
    expect([critical.tone, critical.sticky]).toEqual(['danger', true]);
  });

  it('let good news go by itself', () => {
    expect(noticeView(notice('ok', 12))).toMatchObject({ tone: 'ok', sticky: false, hint: '' });
  });
});
