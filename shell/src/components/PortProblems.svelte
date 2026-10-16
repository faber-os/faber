<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { groupPortProblems } from '../lib/state/portHints';
  import { t } from '../lib/i18n';
  import { useDesktop } from './desktopController.svelte';
  import DismissButton from './DismissButton.svelte';
  import Icon from './Icon.svelte';

  /**
   * Things plugged in over USB that never became a port, so an empty port list isn't a mystery.
   * One line each; what happened and what to try open on request, so it doesn't fill the window.
   * Each kind can be dismissed (×) until one more of it turns up; Parts, Add part and the Serial
   * monitor share the dismissal.
   */
  const core = useCore();
  const desktop = useDesktop();
  const groups = $derived(
    groupPortProblems(core.portProblems).map((p) => ({
      ...p,
      notice: `portProblems:${p.code}`,
      items: Array<string>(p.count).fill(p.code),
    })),
  );
  const shown = $derived(groups.filter((p) => !desktop.isDismissed(p.notice, p.items)));
</script>

{#each shown as p (p.code)}
  <div class="problem" role="status" data-code={p.code}>
    <span class="ico"><Icon name="warning" size={18} /></span>
    <div class="text">
      <p class="label">
        {p.label}{#if p.count > 1}{' '}<span class="count num"
            >{t('portProblems.count', { count: p.count })}</span
          >{/if}
      </p>
      <details>
        <summary>{t('portProblems.whatToTry')}</summary>
        <p class="detail">{p.detail}</p>
        <p class="hint">{p.hint}</p>
      </details>
    </div>
    <DismissButton ondismiss={() => desktop.dismiss(p.notice, p.items)} />
  </div>
{/each}

<style>
  .problem {
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
    padding: var(--space-3) var(--space-2) var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--warn-soft);
    user-select: text;
  }
  .ico {
    display: inline-flex;
    margin-top: 2px;
    color: var(--warn-text);
  }
  .text {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }
  .label {
    font-weight: 700;
  }
  .count {
    font-weight: 500;
    color: var(--text-muted);
  }
  .detail,
  .hint {
    font-size: var(--text-sm);
  }
  summary {
    width: fit-content;
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--warn-text);
    cursor: pointer;
  }
  details[open] summary {
    margin-bottom: var(--space-1);
  }
  .detail {
    color: var(--text-muted);
  }
</style>
