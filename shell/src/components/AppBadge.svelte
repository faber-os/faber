<script lang="ts">
  import { t } from '../lib/i18n';

  /**
   * Marks a derived signal (D24): computed by an app and published under its name
   * (`app:<id>/<name>`), read-only, never a part's own. Shown wherever signals are listed.
   */
  let { app }: { app: string } = $props();
</script>

<span class="badge" title={t('signals.appBadge.title', { app })}>{t('signals.appBadge')}</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    height: 18px;
    padding: 0 6px;
    border-radius: var(--radius-sm);
    background: var(--info-soft);
    color: var(--info);
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.02em;
    text-transform: lowercase;
    vertical-align: middle;
  }
</style>
