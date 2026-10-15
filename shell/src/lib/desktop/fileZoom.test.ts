import { describe, expect, it } from 'vitest';
import { DEFAULT_VIEW, keyZoom, pinchZoom, wheelZoom, zoomView, type FileView } from './fileZoom';

describe('file view zoom', () => {
  it('steps details → small icons → large icons and stops at the ends', () => {
    expect(zoomView('details', 1)).toBe('small');
    expect(zoomView('small', 1)).toBe('large');
    expect(zoomView('large', 1)).toBe('large');
    expect(zoomView('large', -1)).toBe('small');
    expect(zoomView('details', -1)).toBe('details');
    expect(DEFAULT_VIEW).toBe('details');
  });

  it('zooms in with the wheel turned up, one step per notch', () => {
    expect(wheelZoom('details', 0, -100)).toEqual({ view: 'small', carried: 0 });
    expect(wheelZoom('small', 0, 100)).toEqual({ view: 'details', carried: 0 });
  });

  it('adds up small trackpad deltas until they make a step', () => {
    let s: { view: FileView; carried: number } = { view: 'details', carried: 0 };
    for (let i = 0; i < 7; i++) s = wheelZoom(s.view, s.carried, -10);
    expect(s).toEqual({ view: 'details', carried: -70 });
    s = wheelZoom(s.view, s.carried, -10);
    expect(s).toEqual({ view: 'small', carried: 0 });
  });

  it('follows Ctrl +, Ctrl − and Ctrl 0', () => {
    expect(keyZoom('details', '+')).toBe('small');
    expect(keyZoom('details', '=')).toBe('small');
    expect(keyZoom('large', '-')).toBe('small');
    expect(keyZoom('large', '0')).toBe('details');
    expect(keyZoom('large', 'a')).toBeNull();
  });

  it('zooms with a pinch once the fingers have moved far enough', () => {
    expect(pinchZoom('details', 100, 110)).toBe('details');
    expect(pinchZoom('details', 100, 130)).toBe('small');
    expect(pinchZoom('large', 100, 75)).toBe('small');
    expect(pinchZoom('large', 0, 75)).toBe('large');
  });
});
