import { describe, expect, it } from 'vitest';
import { apiFingerprint } from './apiFingerprint.ts';

describe('apiFingerprint', () => {
  it('matches the core (core/tests/api_fingerprint.rs), ignoring CRLF and non-.ts files', () => {
    expect(apiFingerprint('../fixtures/api-fingerprint')).toBe('94e7e7bc99bf6f23');
  });
});
