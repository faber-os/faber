import type { ErrorBody } from './types';

/** What a request to the core gave: its answer, or what went wrong and what to try. */
export type Result<T> = { ok: true; value: T } | { ok: false; error: ErrorBody };
