export interface FakeRoute {
  method?: string;
  path: string;
  status?: number;
  body?: unknown;
  /** Raw text body instead of JSON. */
  text?: string;
  /** Make fetch itself reject, as it does when the core cannot be reached. */
  networkError?: boolean;
}

export interface FakeFetch {
  fetch: typeof fetch;
  calls: { method: string; path: string; body: unknown }[];
  route(r: FakeRoute): void;
}

/** A fetch stand-in that answers from a list of routes; the last matching route wins. */
export function fakeFetch(routes: FakeRoute[] = []): FakeFetch {
  const table = [...routes];
  const calls: FakeFetch['calls'] = [];
  const fn = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const path = String(input);
    const method = (init?.method ?? 'GET').toUpperCase();
    const body = typeof init?.body === 'string' ? JSON.parse(init.body) : undefined;
    calls.push({ method, path, body });
    const match = [...table]
      .reverse()
      .find((r) => r.path === path && (r.method ?? 'GET') === method);
    if (!match) return new Response('not found', { status: 404 });
    if (match.networkError) throw new TypeError('Failed to fetch');
    const status = match.status ?? 200;
    if (status === 204) return new Response(null, { status });
    if (match.text !== undefined) return new Response(match.text, { status });
    return new Response(JSON.stringify(match.body ?? null), {
      status,
      headers: { 'content-type': 'application/json' },
    });
  };
  return { fetch: fn as typeof fetch, calls, route: (r) => table.push(r) };
}
