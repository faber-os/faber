<script lang="ts">
  import { useCore } from '../../lib/state/context';
  import { roleName, rolesUpTo } from '../../lib/state/session.svelte';
  import type { Result } from '../../lib/state/result';
  import type {
    ErrorBody,
    TokenCreated,
    TokenInfo,
    UserInfo,
    UserRole,
  } from '../../lib/state/types';
  import Dialog from '../../components/Dialog.svelte';
  import ErrorNotice from '../../components/ErrorNotice.svelte';
  import RoleLock from '../../components/RoleLock.svelte';
  import RichText from '../../components/RichText.svelte';
  import { t } from '../../lib/i18n';

  /**
   * Settings → Users (docs/auth.md): your password, the machine's users (admins), and API tokens
   * for programs. What a role can't do is shown disabled with the reason, never hidden.
   */
  let { winId }: { winId: string } = $props();

  const ROLES: UserRole[] = ['viewer', 'operator', 'admin'];

  const core = useCore();
  const auth = core.auth;
  const identity = $derived(auth.info?.identity ?? null);
  const me = $derived(auth.user);
  const bySession = $derived(identity?.via === 'session');
  const adminLock = $derived(auth.lock('admin'));
  const tokenLock = $derived(bySession ? null : t('settings.users.tokens.needsSession'));

  let users = $state<UserInfo[] | null>(null);
  let tokens = $state<TokenInfo[] | null>(null);
  let error = $state<ErrorBody | null>(null);
  let done = $state<string | null>(null);
  let busy = $state(false);

  let current = $state('');
  let next = $state('');
  let repeat = $state('');
  const mismatch = $derived(repeat !== '' && repeat !== next);

  let newName = $state('');
  let newPassword = $state('');
  let newRole = $state<UserRole>('viewer');

  let resetFor = $state<string | null>(null);
  let resetPassword = $state('');
  let removing = $state<string | null>(null);

  let tokenName = $state('');
  let tokenRole = $state<UserRole>('viewer');
  let created = $state<TokenCreated | null>(null);
  let copied = $state(false);

  async function load() {
    if (auth.can('admin')) {
      const u = await auth.users();
      if (u.ok) users = u.value;
      else error = u.error;
    }
    if (identity && identity.via !== 'screen') {
      const tk = await auth.tokens();
      if (tk.ok) tokens = tk.value;
      else error = tk.error;
    }
  }

  $effect(() => {
    void auth.role;
    void load();
  });

  /** Runs a change, then shows what it said or why it failed, and reads the lists again. */
  async function run(fn: () => Promise<Result<unknown>>, message: string) {
    busy = true;
    error = null;
    done = null;
    const r = await fn();
    busy = false;
    if (r.ok) done = message;
    else error = r.error;
    await load();
    return r.ok;
  }

  async function changeOwn(e: SubmitEvent) {
    e.preventDefault();
    if (!me || mismatch) return;
    const ok = await run(
      () => auth.setPassword(me, next, current),
      t('settings.users.password.changed'),
    );
    if (ok) current = next = repeat = '';
  }

  async function addUser(e: SubmitEvent) {
    e.preventDefault();
    const name = newName.trim().toLowerCase();
    const ok = await run(
      () => auth.addUser(name, newPassword, newRole),
      t('settings.users.add.done', { name }),
    );
    if (ok) newName = newPassword = '';
  }

  function setRole(name: string, role: UserRole) {
    void run(
      () => auth.setRole(name, role),
      t('settings.users.role.done', { name, role: roleName(role) }),
    );
  }

  async function reset() {
    const name = resetFor;
    if (!name) return;
    const ok = await run(
      () => auth.setPassword(name, resetPassword),
      t('settings.users.reset.done', { name }),
    );
    if (ok) {
      resetFor = null;
      resetPassword = '';
    }
  }

  async function remove() {
    const name = removing;
    removing = null;
    if (name) await run(() => auth.removeUser(name), t('settings.users.remove.done', { name }));
  }

  async function createToken(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    done = null;
    copied = false;
    const r = await auth.createToken(tokenName.trim(), tokenRole);
    busy = false;
    if (r.ok) {
      created = r.value;
      tokenName = '';
    } else error = r.error;
    await load();
  }

  async function copy() {
    if (!created) return;
    await navigator.clipboard?.writeText(created.secret);
    copied = true;
  }

  function revoke(tk: TokenInfo) {
    void run(() => auth.revokeToken(tk.id), t('settings.users.tokens.revoked', { name: tk.name }));
  }
</script>

<section id="{winId}-users" aria-labelledby="{winId}-users-h">
  <h3 id="{winId}-users-h">{t('settings.users.title')}</h3>
  {#if error}<ErrorNotice {error} />{/if}
  {#if done}<p class="done" role="status">{done}</p>{/if}

  <div class="row">
    <div class="text">
      <p class="name">{t('settings.users.you.name')}</p>
      <p class="desc">
        {#if identity?.via === 'screen'}
          {t('settings.users.you.screen', { role: roleName(identity.role) })}
        {:else if identity?.via === 'developer'}
          {t('settings.users.you.off')}
        {:else if identity && me}
          <RichText
            text={t('settings.users.you.user', { name: me, role: roleName(identity.role) })}
          />
        {/if}
      </p>
    </div>
  </div>

  {#if bySession && me}
    <form class="block" onsubmit={changeOwn} aria-labelledby="{winId}-pw-h">
      <p class="name" id="{winId}-pw-h">{t('settings.users.password.name')}</p>
      <div class="fields">
        <label class="field">
          <span class="label">{t('settings.users.password.current')}</span>
          <input
            class="input"
            type="password"
            autocomplete="current-password"
            bind:value={current}
            required
          />
        </label>
        <label class="field">
          <span class="label">{t('settings.users.password.new')}</span>
          <input
            class="input"
            type="password"
            autocomplete="new-password"
            bind:value={next}
            required
          />
        </label>
        <label class="field">
          <span class="label">{t('auth.password.repeat')}</span>
          <input
            class="input"
            type="password"
            autocomplete="new-password"
            bind:value={repeat}
            aria-invalid={mismatch}
            required
          />
        </label>
      </div>
      {#if mismatch}<p class="problem" role="alert">{t('auth.password.mismatch')}</p>{/if}
      <p class="desc">{t('settings.users.password.desc')}</p>
      <div>
        <button class="btn" disabled={busy || mismatch}>{t('settings.users.password.save')}</button>
      </div>
    </form>
  {/if}

  <div class="block">
    <p class="name" id="{winId}-users-list">{t('settings.users.list.name')}</p>
    <RoleLock reason={adminLock} />
    {#if users}
      <ul class="list" aria-labelledby="{winId}-users-list">
        {#each users as u (u.name)}
          <li>
            <span class="who">{u.name}</span>
            <select
              class="input"
              aria-label={t('settings.users.role.label', { name: u.name })}
              value={u.role}
              disabled={busy}
              onchange={(e) => setRole(u.name, e.currentTarget.value as UserRole)}
            >
              {#each ROLES as r (r)}<option value={r}>{roleName(r)}</option>{/each}
            </select>
            <button
              class="btn quiet"
              disabled={busy}
              onclick={() => {
                resetFor = u.name;
                resetPassword = '';
              }}>{t('settings.users.reset.button')}</button
            >
            <button
              class="btn quiet danger"
              disabled={busy || u.name === me}
              title={u.name === me ? t('settings.users.remove.yourself') : undefined}
              aria-label={t('settings.users.remove.label', { name: u.name })}
              onclick={() => (removing = u.name)}>{t('settings.users.remove.button')}</button
            >
          </li>
        {/each}
      </ul>
    {/if}
    <form class="fields" onsubmit={addUser} aria-label={t('settings.users.add.label')}>
      <label class="field">
        <span class="label">{t('auth.name')}</span>
        <input
          class="input"
          bind:value={newName}
          autocomplete="off"
          spellcheck="false"
          required
          disabled={adminLock !== null}
        />
      </label>
      <label class="field">
        <span class="label">{t('auth.password')}</span>
        <input
          class="input"
          type="password"
          autocomplete="new-password"
          bind:value={newPassword}
          required
          disabled={adminLock !== null}
        />
      </label>
      <label class="field">
        <span class="label">{t('settings.users.role')}</span>
        <select class="input" bind:value={newRole} disabled={adminLock !== null}>
          {#each ROLES as r (r)}<option value={r}>{roleName(r)}</option>{/each}
        </select>
      </label>
      <button
        class="btn primary add"
        disabled={busy || adminLock !== null}
        title={adminLock ?? undefined}>{t('settings.users.add.button')}</button
      >
    </form>
    <p class="desc">{t('settings.users.roles.desc')}</p>
  </div>

  <div class="block">
    <p class="name" id="{winId}-tokens">{t('settings.users.tokens.name')}</p>
    <p class="desc"><RichText text={t('settings.users.tokens.desc')} /></p>
    <RoleLock reason={tokenLock} />
    {#if created}
      <div class="secret" role="status">
        <p class="name">{t('settings.users.tokens.created', { name: created.token.name })}</p>
        <p class="desc">{t('settings.users.tokens.once')}</p>
        <div class="secret-row">
          <input
            class="input mono"
            readonly
            value={created.secret}
            aria-label={t('settings.users.tokens.secret')}
            onfocus={(e) => e.currentTarget.select()}
          />
          <button class="btn" onclick={copy}
            >{copied ? t('settings.users.tokens.copied') : t('settings.users.tokens.copy')}</button
          >
          <button class="btn quiet" onclick={() => (created = null)}
            >{t('settings.users.tokens.hide')}</button
          >
        </div>
      </div>
    {/if}
    {#if tokens && tokens.length > 0}
      <ul class="list" aria-labelledby="{winId}-tokens">
        {#each tokens as tk (tk.id)}
          <li>
            <span class="who">{tk.name}</span>
            <span class="desc"
              >{t('settings.users.tokens.row', {
                user: tk.user,
                role: roleName(tk.role),
                created: tk.created.slice(0, 10),
              })}</span
            >
            <button
              class="btn quiet danger"
              disabled={busy}
              aria-label={t('settings.users.tokens.revokeLabel', { name: tk.name })}
              onclick={() => revoke(tk)}>{t('settings.users.tokens.revoke')}</button
            >
          </li>
        {/each}
      </ul>
    {:else if tokens}
      <p class="desc">{t('settings.users.tokens.none')}</p>
    {/if}
    <form class="fields" onsubmit={createToken} aria-label={t('settings.users.tokens.new')}>
      <label class="field">
        <span class="label">{t('settings.users.tokens.tokenName')}</span>
        <input
          class="input"
          bind:value={tokenName}
          placeholder={t('settings.users.tokens.placeholder')}
          maxlength="64"
          required
          disabled={tokenLock !== null}
        />
      </label>
      <label class="field">
        <span class="label">{t('settings.users.role')}</span>
        <select class="input" bind:value={tokenRole} disabled={tokenLock !== null}>
          {#each rolesUpTo(identity?.role ?? 'viewer') as r (r)}<option value={r}
              >{roleName(r)}</option
            >{/each}
        </select>
      </label>
      <button
        class="btn primary add"
        disabled={busy || tokenLock !== null}
        title={tokenLock ?? undefined}>{t('settings.users.tokens.create')}</button
      >
    </form>
  </div>
</section>

{#if resetFor}
  {@const name = resetFor}
  <Dialog title={t('settings.users.reset.title', { name })} oncancel={() => (resetFor = null)}>
    <p>{t('settings.users.reset.body', { name })}</p>
    <label class="field">
      <span class="label">{t('settings.users.password.new')}</span>
      <input class="input" type="password" autocomplete="new-password" bind:value={resetPassword} />
    </label>
    {#snippet actions()}
      <button class="btn" onclick={() => (resetFor = null)}>{t('auth.cancel')}</button>
      <button class="btn primary" disabled={busy || resetPassword === ''} onclick={reset}
        >{t('settings.users.reset.confirm')}</button
      >
    {/snippet}
  </Dialog>
{/if}

{#if removing}
  {@const name = removing}
  <Dialog
    title={t('settings.users.remove.title', { name })}
    tone="danger"
    oncancel={() => (removing = null)}
  >
    <p>{t('settings.users.remove.body', { name })}</p>
    {#snippet actions()}
      <button class="btn" onclick={() => (removing = null)}>{t('auth.cancel')}</button>
      <button class="btn danger solid" onclick={remove}>{t('settings.users.remove.confirm')}</button
      >
    {/snippet}
  </Dialog>
{/if}

<style>
  .block {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3) 0;
    border-top: 1px solid var(--line);
  }
  .fields {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: var(--space-2);
  }
  .fields .field {
    flex: 1 1 9rem;
    min-width: 0;
  }
  .add {
    flex: none;
  }
  .list {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .list li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) 0;
  }
  .who {
    flex: 1 1 8rem;
    font-weight: 650;
    overflow-wrap: anywhere;
  }
  .secret {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--ok-soft, var(--surface-2));
  }
  .secret-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .secret-row .input {
    flex: 1 1 16rem;
    min-width: 0;
  }
  .mono {
    font-family: var(--font-mono);
  }
  .done {
    color: var(--ok, var(--text));
    font-size: var(--text-sm);
  }
  .problem {
    font-size: var(--text-sm);
    color: var(--danger);
  }
</style>
