import { describe, expect, it } from 'vitest';
import { fixWords, hasFix, latitudeWords, longitudeWords, mapLink } from './position';

describe('position', () => {
  it('writes coordinates with their hemisphere', () => {
    expect(latitudeWords(55.676098)).toBe('55.676098° N');
    expect(latitudeWords(-33.8688)).toBe('33.868800° S');
    expect(longitudeWords(12.568337)).toBe('12.568337° E');
    expect(longitudeWords(-0.1276)).toBe('0.127600° W');
  });

  it('says how good the fix is', () => {
    expect(hasFix('none')).toBe(false);
    expect(hasFix('fix3d')).toBe(true);
    expect(fixWords('fix2d')).toBe('2D (no altitude)');
    expect(fixWords('rtk-fixed')).toBe('RTK fixed');
  });

  it('links to the place on OpenStreetMap', () => {
    expect(mapLink(55.676098, -12.5)).toBe(
      'https://www.openstreetmap.org/?mlat=55.676098&mlon=-12.500000#map=17/55.676098/-12.500000',
    );
  });
});
