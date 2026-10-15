<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { t, type MessageKey } from '../lib/i18n';
  import type { ConnectionStatus } from '../lib/state/types';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import RichText from '../components/RichText.svelte';
  import type { WindowProps } from '../components/windowDefs';
  import Wordmark from '../components/Wordmark.svelte';

  // Every window is given these (see windowDefs.ts); this one does not need them.
  let props: WindowProps = $props();

  const core = useCore();
  const host = typeof location === 'undefined' ? '' : location.host;

  const STATUS: Record<ConnectionStatus, Extract<MessageKey, `about.status.${string}`>> = {
    connected: 'about.status.connected',
    connecting: 'about.status.connecting',
    reconnecting: 'about.status.reconnecting',
    closed: 'about.status.closed',
  };
</script>

<div class="about">
  <div class="hero">
    <Wordmark size={40} os />
    <p class="tagline"><RichText text={t('about.tagline')} /></p>
  </div>

  {#if core.systemError}
    <ErrorNotice error={core.systemError}>
      {#snippet actions()}
        <button class="btn" onclick={() => core.loadSystem()}>{t('about.tryAgain')}</button>
      {/snippet}
    </ErrorNotice>
  {/if}

  <dl class="facts">
    <dt>{t('about.core')}</dt>
    <dd>
      {#if core.system}{core.system.name} <span class="num">{core.system.version}</span>{:else}{t(
          'about.core.unknown',
        )}{/if}
    </dd>
    <dt>{t('about.shell')}</dt>
    <dd class="num">{SHELL_VERSION}</dd>
    <dt>{t('about.address')}</dt>
    <dd><code>{host}</code></dd>
    <dt>{t('about.connection')}</dt>
    <dd>
      <RichText
        text={t('about.connection.detail', {
          status: t(STATUS[core.connection]),
          format: core.format,
        })}
      />
    </dd>
  </dl>

  <p class="licence">{t('about.licence')}</p>
</div>

<style>
  .about {
    display: grid;
    align-content: start;
    gap: var(--space-5);
    flex: 1;
    overflow-y: auto;
    padding: var(--space-6) var(--space-5) var(--space-5);
    background: var(--surface);
    user-select: text;
  }
  .hero {
    display: grid;
    gap: var(--space-3);
  }
  .tagline {
    max-width: 40ch;
    color: var(--text-muted);
  }
  .tagline :global(strong) {
    font-style: normal;
    font-weight: 650;
    color: var(--text);
  }
  .facts {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-2) var(--space-5);
    margin: 0;
    padding-top: var(--space-4);
    border-top: 1px solid var(--line);
    font-size: var(--text-sm);
  }
  dt {
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    font-weight: 560;
  }
  .licence {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
</style>
