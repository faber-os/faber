// @faber-os/shell 0.1.0: the SDK for a Faber app's UI (docs/apps.md).
// Built from sdk/js/shell/src/faber-shell.ts by `npm run build`; edit that file, not this one.
/** The protocol version this SDK speaks. */
export const PROTOCOL = 1;
/**
 * Something the shell (or the SDK) said no to, in Faber's house style: `message` says what
 * happened, `hint` what to try next, and `code` is stable for programs (`unknown-role`,
 * `lease-held`, `estop-engaged`, …).
 */
export class FaberError extends Error {
    code;
    hint;
    constructor(body) {
        super(body.message);
        this.name = 'FaberError';
        this.code = body.code;
        this.hint = body.hint;
    }
}
/** A lease must be renewed within 250 ms; every 100 ms survives one late beat. */
const RENEW_MS = 100;
function noShell() {
    return new FaberError({
        code: 'no-shell',
        message: "The Faber shell didn't answer.",
        hint: 'Open this app from Faber (its window in the shell); its page does nothing on its own.',
    });
}
function isObject(v) {
    return typeof v === 'object' && v !== null;
}
/** Connects to the shell around this page. Most apps use the ready-made `faber` instead. */
export function connect(options = {}) {
    const parent = options.parent ?? globalThis.parent;
    const self = options.self ?? globalThis;
    const timeoutMs = options.timeoutMs ?? 5000;
    let nextId = 1;
    const pending = new Map();
    const subscriptions = new Map();
    const drives = new Map();
    const statusWatchers = new Set();
    const themeWatchers = new Set();
    let hello = null;
    let closed = false;
    function request(req) {
        if (closed)
            return Promise.reject(noShell());
        const id = nextId++;
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                pending.delete(id);
                reject(noShell());
            }, timeoutMs);
            pending.set(id, { resolve: resolve, reject, timer });
            // The shell's origin can't be named from an opaque-origin page; nothing sent here is secret,
            // and only the shell may frame an app's page (frame-ancestors).
            parent.postMessage({ faber: PROTOCOL, id, ...req }, '*');
        });
    }
    function onMessage(event) {
        if (event.source !== parent)
            return;
        const m = event.data;
        if (!isObject(m) || m.faber !== PROTOCOL)
            return;
        if ('re' in m) {
            const p = pending.get(m.re);
            if (!p)
                return;
            pending.delete(m.re);
            clearTimeout(p.timer);
            if (m.ok)
                p.resolve(m.value);
            else
                p.reject(new FaberError(m.error));
            return;
        }
        switch (m.event) {
            case 'signal':
                subscriptions.get(m.subscription)?.onReading(m.reading);
                break;
            case 'signal-ended': {
                const s = subscriptions.get(m.subscription);
                subscriptions.delete(m.subscription);
                s?.onError?.(new FaberError(m.error));
                break;
            }
            case 'status':
                for (const w of [...statusWatchers])
                    w(m.status);
                break;
            case 'theme':
                for (const w of [...themeWatchers])
                    w(m.theme);
                break;
            case 'lease-ended':
                drives.get(key(m.role, m.index))?.lost(new FaberError(m.error));
                break;
        }
    }
    self.addEventListener('message', onMessage);
    function watch(source, callback, opts = {}) {
        let id = null;
        let stopped = false;
        const req = { type: 'subscribe', source };
        if (opts.maxHz !== undefined)
            req.maxHz = opts.maxHz;
        request(req).then(({ subscription }) => {
            if (stopped) {
                void request({ type: 'unsubscribe', subscription }).catch(() => undefined);
                return;
            }
            id = subscription;
            subscriptions.set(subscription, { onReading: callback, onError: opts.onError });
        }, (e) => opts.onError?.(e));
        return () => {
            if (stopped)
                return;
            stopped = true;
            if (id === null)
                return;
            subscriptions.delete(id);
            void request({ type: 'unsubscribe', subscription: id }).catch(() => undefined);
        };
    }
    function key(role, index) {
        return `${role}#${index}`;
    }
    class DriveHandle {
        role;
        index;
        ended = false;
        callbacks = [];
        timer;
        lastSent = Date.now();
        constructor(role, index) {
            this.role = role;
            this.index = index;
            // The page's own heartbeat keeps the lease: a page that hangs lets it lapse, and the part stops.
            this.timer = setInterval(() => this.beat(), RENEW_MS);
        }
        async command(command) {
            if (this.ended)
                throw this.endedError();
            this.lastSent = Date.now();
            return request({ type: 'command', role: this.role, index: this.index, command });
        }
        async release() {
            if (this.ended)
                return;
            this.finish();
            await request({ type: 'release', role: this.role, index: this.index });
        }
        onEnd(callback) {
            this.callbacks.push(callback);
        }
        lost(error) {
            if (this.ended)
                return;
            this.finish();
            for (const c of this.callbacks)
                c(error);
        }
        beat() {
            if (Date.now() - this.lastSent < RENEW_MS - 5)
                return;
            this.lastSent = Date.now();
            request({ type: 'renew', role: this.role, index: this.index }).catch((e) => {
                if (['lease-ended', 'lease-required', 'no-shell'].includes(e.code))
                    this.lost(e);
            });
        }
        finish() {
            this.ended = true;
            if (this.timer !== null)
                clearInterval(this.timer);
            this.timer = null;
            drives.delete(key(this.role, this.index));
        }
        endedError() {
            return new FaberError({
                code: 'lease-ended',
                message: `This app no longer drives "${this.role}".`,
                hint: 'Call faber.drive() again to drive it.',
            });
        }
    }
    const api = {
        hello() {
            hello ??= request({ type: 'hello' }).catch((e) => {
                hello = null;
                throw e;
            });
            return hello;
        },
        roles: () => request({ type: 'roles' }),
        latest: (role, opts = {}) => request({ type: 'latest', source: { role, index: opts.index ?? 0 } }),
        subscribe: (role, callback, opts = {}) => watch({ role, index: opts.index ?? 0 }, callback, opts),
        published: (name) => request({ type: 'latest', source: { published: name } }),
        subscribePublished: (name, callback, opts = {}) => watch({ published: name }, callback, opts),
        async drive(role, opts = {}) {
            const index = opts.index ?? 0;
            const existing = drives.get(key(role, index));
            if (existing)
                return existing;
            await request({ type: 'drive', role, index });
            const handle = new DriveHandle(role, index);
            drives.set(key(role, index), handle);
            return handle;
        },
        status: () => request({ type: 'status' }),
        onStatus(callback) {
            statusWatchers.add(callback);
            return () => void statusWatchers.delete(callback);
        },
        theme: () => request({ type: 'theme' }),
        onTheme(callback) {
            themeWatchers.add(callback);
            return () => void themeWatchers.delete(callback);
        },
        close() {
            closed = true;
            self.removeEventListener('message', onMessage);
            for (const [, p] of pending) {
                clearTimeout(p.timer);
                p.reject(noShell());
            }
            pending.clear();
            for (const d of [...drives.values()])
                d.lost(noShell());
        },
    };
    return api;
}
/**
 * Makes the page look like the shell: sets every theme token (`--surface`, `--text`, `--accent`,
 * `--font-ui`, …) on `root` and its `color-scheme`. Use the tokens in your CSS:
 * `background: var(--surface); color: var(--text);`.
 */
export function applyTheme(theme, root) {
    const el = root ??
        globalThis.document.documentElement;
    for (const [name, value] of Object.entries(theme.tokens)) {
        if (/^--[a-z0-9-]+$/.test(name))
            el.style.setProperty(name, value);
    }
    el.style.setProperty('color-scheme', theme.scheme);
}
let shared = null;
/** The connection to the shell around this page, made on first use. */
export const faber = new Proxy({}, {
    get(_, name) {
        shared ??= connect();
        const value = shared[name];
        return typeof value === 'function' ? value.bind(shared) : value;
    },
});
