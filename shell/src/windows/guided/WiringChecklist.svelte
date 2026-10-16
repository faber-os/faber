<script lang="ts" module>
  let count = 0;
</script>

<script lang="ts">
  import { t } from '../../lib/i18n';
  import { browserStorage } from '../../components/desktopController.svelte';
  import { lineId, loadTicks, saveTicks } from '../../lib/desktop/wiringChecklist';
  import { wireChangeText, type WireChange } from '../../lib/state/guided';
  import type { GuidedBoard, PartTemplate } from '../../lib/state/types';
  import Icon from '../../components/Icon.svelte';

  /**
   * The wiring checklist (docs/wiring.md §3, step 4): each part's wires with the pins Faber
   * chose, power warnings first, a box to tick per wire (kept in this browser per board), and the
   * wires that change when the plan was edited. Prints on its own.
   */
  let {
    board,
    templates = [],
    changes = [],
    onpopout,
  }: {
    board: GuidedBoard;
    templates?: PartTemplate[];
    /** The wires that changed with the last save; shown when one moved or went. */
    changes?: WireChange[];
    /** "Open in a window": absent in that window itself. */
    onpopout?: () => void;
  } = $props();

  const id = `checklist-${++count}`;
  const storage = browserStorage();
  const lines = $derived(board.wiring.flatMap((w) => w.lines.map((l) => lineId(w.part, l))));
  let ticks = $state(new Set<string>());
  $effect(() => {
    ticks = loadTicks(storage, board.name);
  });
  const done = $derived(lines.filter((l) => ticks.has(l)).length);
  /** Problems worth knowing while wiring: power, logic levels, interrupts. */
  const warnings = $derived(board.problems.filter((p) => p.severity === 'warning'));

  function toggle(line: string, on: boolean) {
    const next = new Set(ticks);
    if (on) next.add(line);
    else next.delete(line);
    ticks = next;
    saveTicks(storage, board.name, next, lines);
  }

  function templateName(id: string): string {
    return templates.find((x) => x.id === id)?.name ?? id;
  }

  let printing = $state(false);
  function print() {
    printing = true;
    document.body.dataset.print = id;
    const done = () => {
      printing = false;
      delete document.body.dataset.print;
      window.removeEventListener('afterprint', done);
    };
    window.addEventListener('afterprint', done);
    window.print();
  }
</script>

<section class="checklist" data-printing={printing || undefined} aria-labelledby="{id}-title">
  <header class="head">
    <div>
      <h4 id="{id}-title">{t('guided.wiring.title', { board: board.name })}</h4>
      <p class="progress" aria-live="polite">
        {t('guided.wiring.progress', { done, count: lines.length })}
      </p>
    </div>
    <div class="actions no-print">
      {#if onpopout}
        <button class="btn" onclick={onpopout}
          ><Icon name="open" size={16} /> {t('guided.wiring.popout')}</button
        >
      {/if}
      <button class="btn" onclick={print}>{t('guided.wiring.print')}</button>
    </div>
  </header>

  {#if warnings.length > 0}
    <ul class="warnings" aria-label={t('guided.wiring.warnings')}>
      {#each warnings as w, i (i)}
        <li>
          <Icon name="warning" size={18} />
          <span><b>{w.message}</b> {w.hint}</span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if changes.some((c) => c.from)}
    <div class="changes">
      <p class="sub">{t('guided.wiring.changes')}</p>
      <ul>
        {#each changes as c, i (i)}
          <li class:gone={!c.to}>{wireChangeText(c)}</li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if board.wiring.length === 0}
    <p class="muted">{t('guided.wiring.empty')}</p>
  {/if}
  {#each board.wiring as w (w.part)}
    <div class="part">
      <p class="part-name">
        <code>{w.part}</code> <span class="muted">{templateName(w.template)}</span>
      </p>
      <ul class="lines">
        {#each w.lines as line (line)}
          {@const lid = lineId(w.part, line)}
          <li>
            <label class="line">
              <input
                class="check"
                type="checkbox"
                checked={ticks.has(lid)}
                onchange={(e) => toggle(lid, e.currentTarget.checked)}
              />
              <span>{line}</span>
            </label>
          </li>
        {/each}
      </ul>
      {#if w.warnings.length > 0}
        <ul class="part-warnings">
          {#each w.warnings as text (text)}<li>{text}</li>{/each}
        </ul>
      {/if}
    </div>
  {/each}
</section>

<style>
  .checklist {
    display: grid;
    gap: var(--space-3);
    min-width: 0;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2);
  }
  h4 {
    font-size: var(--text-md);
    font-weight: 680;
  }
  .progress {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .warnings {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-3);
    list-style: none;
    border-radius: var(--radius-md);
    background: var(--warn-soft);
    border-left: 3px solid var(--warn);
  }
  .warnings li {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    font-size: var(--text-sm);
  }
  .warnings :global(svg) {
    flex: none;
    color: var(--warn-text);
  }
  .changes {
    padding: var(--space-2) var(--space-3);
    border: 1px dashed var(--accent);
    border-radius: var(--radius-md);
  }
  .changes ul {
    margin: var(--space-1) 0 0;
    padding-left: var(--space-5);
    font-family: var(--font-data);
    font-size: var(--text-sm);
  }
  .changes .gone {
    text-decoration: line-through;
    color: var(--text-muted);
  }
  .sub {
    font-size: var(--text-sm);
    font-weight: 650;
  }
  .part {
    display: grid;
    gap: var(--space-1);
  }
  .part-name {
    font-weight: 650;
  }
  .lines {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 2px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 34px;
    font-family: var(--font-data);
    font-size: var(--text-sm);
    cursor: pointer;
  }
  .line:has(input:checked) span {
    color: var(--text-muted);
    text-decoration: line-through;
  }
  .part-warnings {
    margin: 0;
    padding-left: var(--space-5);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .muted {
    color: var(--text-muted);
    font-weight: 400;
  }
  @media print {
    :global(body[data-print] *) {
      visibility: hidden;
    }
    .checklist[data-printing],
    .checklist[data-printing] :global(*) {
      visibility: visible;
    }
    .checklist[data-printing] {
      position: fixed;
      inset: 0;
      padding: 1cm;
      background: white;
      color: black;
    }
    .checklist[data-printing] .no-print {
      display: none;
    }
  }
</style>
