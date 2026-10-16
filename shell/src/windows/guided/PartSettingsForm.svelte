<script lang="ts">
  import { t } from '../../lib/i18n';
  import { readSetting } from '../../lib/state/guided';
  import type { PartTemplate } from '../../lib/state/types';

  /**
   * A part's settings, as its template describes them (docs/wiring.md §11): units, defaults,
   * limits and help, checked while typing. Each says whether it changes without an upload.
   */
  let {
    template,
    values = $bindable(),
    idPrefix = 'guided-setting',
  }: {
    template: PartTemplate;
    /** Setting id → the text in its field (a choice's id for a choice). */
    values: Record<string, string>;
    idPrefix?: string;
  } = $props();

  const problems = $derived(
    Object.fromEntries(
      template.settings.map((s) => {
        const r = readSetting(s, values[s.id] ?? '');
        return [s.id, r.ok ? null : r.message];
      }),
    ) as Record<string, string | null>,
  );
</script>

{#if template.settings.length > 0}
  <div class="settings">
    {#each template.settings as s (s.id)}
      {@const id = `${idPrefix}-${s.id}`}
      {@const problem = problems[s.id]}
      <label class="label" for={id}>{s.name}</label>
      <span class="value">
        {#if s.type === 'choice'}
          <select
            {id}
            class="select"
            value={values[s.id]}
            aria-describedby="{id}-help"
            onchange={(e) => (values = { ...values, [s.id]: e.currentTarget.value })}
          >
            {#each s.options as o (o.id)}<option value={o.id}>{o.name}</option>{/each}
          </select>
        {:else}
          <input
            {id}
            class="input num"
            type="text"
            inputmode={s.type === 'integer' ? 'numeric' : 'decimal'}
            value={values[s.id]}
            aria-invalid={problem !== null}
            aria-describedby="{id}-help"
            oninput={(e) => (values = { ...values, [s.id]: e.currentTarget.value })}
          />
          {#if s.unit}<span class="unit">{s.unit}</span>{/if}
        {/if}
        <span class="apply" title={t(`guided.setting.apply.${s.apply}.title`)}
          >{t(`guided.setting.apply.${s.apply}`)}</span
        >
      </span>
      <p
        class="help wide"
        id="{id}-help"
        class:bad={problem !== null}
        role={problem ? 'alert' : undefined}
      >
        {problem ?? s.help ?? ''}
      </p>
    {/each}
  </div>
{/if}

<style>
  .settings {
    display: grid;
    grid-template-columns: minmax(0, max-content) minmax(0, 1fr);
    align-items: center;
    gap: var(--space-1) var(--space-4);
  }
  .label {
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .value {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .input.num {
    width: 8rem;
    max-width: 100%;
  }
  .unit {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .apply {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .wide {
    grid-column: 1 / -1;
    margin-bottom: var(--space-2);
  }
  .help {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .help.bad {
    color: var(--danger-text);
    font-weight: 600;
  }
  @media (max-width: 480px) {
    .settings {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
