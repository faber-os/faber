<script lang="ts">
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody } from '../lib/state/types';
  import ErrorNotice from './ErrorNotice.svelte';
  import Icon from './Icon.svelte';
  import Wordmark from './Wordmark.svelte';

  /**
   * Signing in (docs/auth.md), in front of everything. `setup` is the first run: while the machine
   * has no users, the first browser creates the admin. The Stop button works here too: stopping is
   * never gated. `oncancel` (the attached screen, which works without signing in) goes back.
   */
  let {
    mode,
    ondone,
    oncancel,
  }: { mode: 'sign-in' | 'setup'; ondone: () => void; oncancel?: () => void } = $props();

  const core = useCore();
  const machine = $derived(core.auth.info?.machineName ?? 'faber');

  let name = $state('');
  let password = $state('');
  let repeat = $state('');
  let busy = $state(false);
  let error = $state<ErrorBody | null>(null);
  let stopped = $state(false);

  const mismatch = $derived(mode === 'setup' && repeat !== '' && repeat !== password);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (busy || mismatch) return;
    busy = true;
    error = null;
    const who = name.trim().toLowerCase();
    const r =
      mode === 'setup'
        ? await core.auth.setup(who, password)
        : await core.auth.signIn(who, password);
    busy = false;
    if (r.ok) {
      password = '';
      repeat = '';
      ondone();
    } else error = r.error;
  }

  async function stop() {
    const r = await core.engageEstop(t('estop.reason.signIn'));
    stopped = r.ok;
  }
</script>

<div class="screen" role="dialog" aria-modal="true" aria-labelledby="sign-in-title">
  <form class="card" onsubmit={submit} aria-labelledby="sign-in-title">
    <div class="mark"><Wordmark size={40} os /></div>
    {#if mode === 'setup'}
      <h1 id="sign-in-title">{t('auth.setup.title')}</h1>
      <p class="lead">{t('auth.setup.lead', { machine })}</p>
    {:else}
      <h1 id="sign-in-title">{t('auth.signIn.title', { machine })}</h1>
    {/if}

    <div class="field">
      <label class="label" for="sign-in-name">{t('auth.name')}</label>
      <input
        id="sign-in-name"
        class="input"
        aria-describedby={mode === 'setup' ? 'sign-in-name-help' : undefined}
        bind:value={name}
        autocomplete="username"
        autocapitalize="none"
        spellcheck="false"
        required
        disabled={busy}
      />
      {#if mode === 'setup'}<span class="help" id="sign-in-name-help">{t('auth.name.help')}</span
        >{/if}
    </div>
    <div class="field">
      <label class="label" for="sign-in-password">{t('auth.password')}</label>
      <input
        id="sign-in-password"
        class="input"
        type="password"
        aria-describedby={mode === 'setup' ? 'sign-in-password-help' : undefined}
        bind:value={password}
        autocomplete={mode === 'setup' ? 'new-password' : 'current-password'}
        required
        disabled={busy}
      />
      {#if mode === 'setup'}<span class="help" id="sign-in-password-help"
          >{t('auth.password.help')}</span
        >{/if}
    </div>
    {#if mode === 'setup'}
      <div class="field">
        <label class="label" for="sign-in-repeat">{t('auth.password.repeat')}</label>
        <input
          id="sign-in-repeat"
          class="input"
          type="password"
          bind:value={repeat}
          autocomplete="new-password"
          required
          disabled={busy}
          aria-invalid={mismatch}
        />
        {#if mismatch}<span class="problem" role="alert">{t('auth.password.mismatch')}</span>{/if}
      </div>
    {/if}

    {#if error}<ErrorNotice {error} />{/if}

    <div class="actions">
      {#if oncancel}
        <button type="button" class="btn" onclick={oncancel}>{t('auth.cancel')}</button>
      {/if}
      <button class="btn primary" disabled={busy || mismatch}>
        {mode === 'setup' ? t('auth.setup.submit') : t('auth.signIn.submit')}
      </button>
    </div>
  </form>

  <div class="stop-row">
    {#if stopped}
      <p class="stopped" role="status"><Icon name="stop" size={16} />{t('estop.engaged')}</p>
    {/if}
    <button type="button" class="stop" title={t('estop.stop.title')} onclick={stop}>
      <Icon name="stop" size={18} />{t('estop.stop')}
    </button>
  </div>
</div>

<style>
  .screen {
    position: fixed;
    inset: 0;
    z-index: 300000;
    display: grid;
    place-items: center;
    padding: var(--space-5);
    background: var(--surface-3);
    overflow-y: auto;
  }
  .card {
    display: grid;
    gap: var(--space-4);
    width: min(24rem, 100%);
    padding: var(--space-6);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow-2, 0 8px 32px rgb(0 0 0 / 0.12));
  }
  .mark {
    justify-self: center;
  }
  h1 {
    margin: 0;
    font-size: var(--text-xl, 1.4rem);
    text-align: center;
  }
  .lead {
    color: var(--text-muted);
    text-align: center;
  }
  .help {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .problem {
    font-size: var(--text-sm);
    color: var(--danger-text);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  .stop-row {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .stop {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: 44px;
    padding: 0 var(--space-4);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--danger);
    color: var(--surface);
    font-weight: 750;
  }
  .stopped {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--danger-text);
    font-weight: 700;
  }
</style>
