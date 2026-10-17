<script lang="ts">
  /**
   * A circuit: parts on the left, the mat in the middle, the inspector on the right, and a toolbar
   * whose File, Add and Code menus come from the window (they talk to the core).
   */
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import Canvas from './Canvas.svelte';
  import CircuitMenu from './CircuitMenu.svelte';
  import Inspector from './Inspector.svelte';
  import Library from './Library.svelte';
  import NewPart from './NewPart.svelte';
  import { useCircuit } from './circuit.svelte';
  import { openMenu, type MenuItem } from './menu';

  let {
    menus,
    onsave,
  }: {
    /** The toolbar's menus, worked out when opened. */
    menus: { file: () => MenuItem[]; add: () => MenuItem[]; code: () => MenuItem[] };
    onsave: () => void;
  } = $props();

  const lab = useCircuit();
  let root: HTMLDivElement;

  const errors = $derived(lab.issues.filter((i) => i.sev === 'error').length);
  const warns = $derived(lab.issues.length - errors);
  const legend = $derived.by(() => {
    const kinds = new Set(
      lab.nets.nets.map((n) =>
        n.kind === 'power' ? (n.volts > 6 ? 'bat' : n.volts < 4 ? '3v3' : '5v') : n.kind,
      ),
    );
    const out: [string, string][] = [];
    if (kinds.has('bat')) out.push(['#be123c', t('circuit.legend.battery')]);
    if (kinds.has('5v')) out.push(['#ef4444', t('circuit.legend.5v')]);
    if (kinds.has('3v3')) out.push(['#f97316', t('circuit.legend.3v3')]);
    if (kinds.has('gnd')) out.push(['#202328', t('circuit.legend.gnd')]);
    if (kinds.has('cable')) out.push(['#8b939c', t('circuit.legend.cables')]);
    out.push(['linear-gradient(90deg,#3b82f6,#facc15,#c084fc)', t('circuit.legend.signals')]);
    return out;
  });
  const MOTORS: ['off' | 'run' | 'stall', () => string][] = [
    ['off', () => t('circuit.motors.off')],
    ['run', () => t('circuit.motors.run')],
    ['stall', () => t('circuit.motors.stall')],
  ];

  onMount(() => {
    requestAnimationFrame(() => lab.fit());
  });

  function onkeydown(e: KeyboardEvent) {
    if ((e.target as Element).matches('input, textarea, select')) return;
    const mod = e.ctrlKey || e.metaKey;
    const k = e.key.toLowerCase();
    const take = () => e.preventDefault();
    if (e.key === 'Escape') {
      if (lab.menu) lab.menu = null;
      else if (lab.connecting) {
        lab.connecting = null;
        lab.hint = '';
      } else if (lab.newPartOpen) lab.newPartOpen = false;
      else lab.clearSelection();
      return;
    }
    if (mod) {
      if (k === 's') (take(), onsave());
      else if (k === 'z' && !e.shiftKey) (take(), lab.undo());
      else if ((k === 'z' && e.shiftKey) || k === 'y') (take(), lab.redo());
      else if (k === 'a') (take(), lab.selectAll());
      else if (k === 'c') (take(), lab.copy());
      else if (k === 'x') (take(), lab.cut());
      else if (k === 'v') (take(), lab.paste());
      else if (k === 'd') (take(), lab.duplicate());
      return;
    }
    if (e.key === 'Delete' || e.key === 'Backspace') return (take(), lab.removeSelection());
    if (k === 'r') return lab.rotate(lab.selParts, e.shiftKey);
    if (k === 'h') return lab.mirror('h');
    if (k === 'v') return lab.mirror('v');
    if (k === 'f') return lab.fit();
    if (k === 'p') return (lab.power = !lab.power);
    if (e.key.startsWith('Arrow') && lab.selParts.length) {
      take();
      const step = e.shiftKey ? 10 : 2;
      const [dx, dy] = (
        {
          ArrowLeft: [-step, 0],
          ArrowRight: [step, 0],
          ArrowUp: [0, -step],
          ArrowDown: [0, step],
        } as Record<string, [number, number]>
      )[e.key]!;
      lab.nudge(dx, dy);
    }
  }

  function menuAt(e: MouseEvent, items: MenuItem[]) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu(lab, r.left, r.bottom + 4, items);
  }

  // Dragging a panel's edge.
  function resize(side: 'left' | 'right', e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const x0 = e.clientX;
    const w0 = lab.panels[side];
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - x0;
      lab.panels[side] = Math.round(
        Math.min(560, Math.max(180, side === 'left' ? w0 + dx : w0 - dx)),
      );
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  const cols = $derived(
    `${lab.panels.leftOpen ? `${lab.panels.left}px 8px` : ''} minmax(0, 1fr) ${lab.panels.rightOpen ? `8px ${lab.panels.right}px` : ''}`.trim(),
  );
  const zoom = (f: number) => {
    const r = lab.el?.getBoundingClientRect();
    if (r) lab.zoomAt(r.width / 2, r.height / 2, f);
  };
</script>

<!-- Keys work while this window has focus; a click anywhere in it gives it focus. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_static_element_interactions -->
<div
  class="app circuit"
  bind:this={root}
  tabindex="-1"
  {onkeydown}
  onpointerdown={(e) =>
    !(e.target as Element).closest('input, textarea, select') &&
    root.focus({ preventScroll: true })}
>
  <header class="bar">
    <button
      class="icon"
      title={lab.panels.leftOpen ? t('circuit.bar.hideParts') : t('circuit.bar.showParts')}
      aria-label={t('circuit.bar.toggleParts')}
      onclick={() => (lab.panels.leftOpen = !lab.panels.leftOpen)}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"
        ><rect
          x="1.5"
          y="2.5"
          width="13"
          height="11"
          rx="2"
          fill="none"
          stroke="currentColor"
        /><rect
          x="2"
          y="3"
          width="4"
          height="10"
          fill={lab.panels.leftOpen ? 'currentColor' : 'none'}
        /></svg
      >
    </button>
    <button class="btn" onclick={(e) => menuAt(e, menus.file())}>{t('circuit.bar.file')}</button>
    <button class="btn" onclick={(e) => menuAt(e, menus.add())}>{t('circuit.bar.add')}</button>
    <button class="btn" onclick={(e) => menuAt(e, menus.code())}>{t('circuit.bar.code')}</button>
    <span class="doc"
      >{t('circuit.bar.counts', { parts: lab.doc.parts.length, wires: lab.doc.wires.length })}</span
    >
    {#if errors}<button class="chip err" onclick={() => (lab.tab = 'checks')}
        >{t('circuit.bar.problems', { count: errors })}</button
      >{:else if warns}<button class="chip warn" onclick={() => (lab.tab = 'checks')}
        >{t('circuit.bar.toLookAt', { count: warns })}</button
      >{:else if lab.doc.parts.length}<span class="chip ok">{t('circuit.bar.ready')}</span>{/if}
    <span class="spacer"></span>
    <div class="tools">
      <button
        class="btn power"
        class:on={lab.power}
        aria-pressed={lab.power}
        title={t('circuit.bar.powerTitle')}
        onclick={() => (lab.power = !lab.power)}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"
          ><path
            d="M8 1.5v6M4.2 3.8a5.5 5.5 0 1 0 7.6 0"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
          /></svg
        >
        {lab.power ? t('circuit.bar.powerOn') : t('circuit.bar.powerOff')}
      </button>
      <button
        class="btn"
        onclick={() => (lab.arrange(), requestAnimationFrame(() => lab.fit()))}
        title={t('circuit.bar.arrangeTitle')}>{t('circuit.bar.arrange')}</button
      >
      <button class="btn" onclick={() => lab.undo()} title={t('circuit.bar.undoTitle')}
        >{t('circuit.bar.undo')}</button
      >
      <button class="btn" onclick={() => lab.redo()} title={t('circuit.bar.redoTitle')}
        >{t('circuit.bar.redo')}</button
      >
      <button class="btn" onclick={() => lab.fit()} title={t('circuit.bar.fitTitle')}
        >{t('circuit.bar.fit')}</button
      >
    </div>
    <button
      class="icon"
      title={lab.panels.rightOpen ? t('circuit.bar.hideInspector') : t('circuit.bar.showInspector')}
      aria-label={t('circuit.bar.toggleInspector')}
      onclick={() => (lab.panels.rightOpen = !lab.panels.rightOpen)}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"
        ><rect
          x="1.5"
          y="2.5"
          width="13"
          height="11"
          rx="2"
          fill="none"
          stroke="currentColor"
        /><rect
          x="10"
          y="3"
          width="4"
          height="10"
          fill={lab.panels.rightOpen ? 'currentColor' : 'none'}
        /></svg
      >
    </button>
  </header>

  <div class="work" style:grid-template-columns={cols}>
    {#if lab.panels.leftOpen}
      <aside class="panel lib" aria-label={t('circuit.library.title')}><Library /></aside>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="handle"
        title={t('circuit.bar.resize')}
        onpointerdown={(e) => resize('left', e)}
      ></div>
    {/if}
    <main class="stage">
      <Canvas />
      {#if lab.hint}<div class="hint">{lab.hint}</div>{/if}
      {#if !lab.doc.parts.length}
        <div class="empty">
          <p>{t('circuit.empty.title')}</p>
          <small>{t('circuit.empty.more')}</small>
          <button class="btn primary" onclick={(e) => menuAt(e, menus.add())}
            >{t('circuit.bar.add')}</button
          >
        </div>
      {/if}
      {#if lab.sim}
        <div class="powerbar">
          {#each lab.sim.messages.slice(0, 4) as m, i (i)}<span class={m.sev}>{m.text}</span>{/each}
          <div class="motors" role="radiogroup" aria-label={t('circuit.motors.label')}>
            {t('circuit.motors.label')}
            {#each MOTORS as [v, text] (v)}
              <button
                role="radio"
                aria-checked={lab.motors === v}
                class:on={lab.motors === v}
                onclick={() => (lab.motors = v)}>{text()}</button
              >
            {/each}
          </div>
          <small>{t('circuit.powerbar.help')}</small>
        </div>
      {/if}
      <div class="legend">
        {#each legend as [c, text] (text)}<span><i style:background={c}></i>{text}</span>{/each}
        <span class="scale">{t('circuit.legend.scale')}</span>
      </div>
      <div class="zoom">
        <button aria-label={t('circuit.bar.zoomOut')} onclick={() => zoom(0.8)}>−</button>
        <button aria-label={t('circuit.bar.zoomIn')} onclick={() => zoom(1.25)}>+</button>
      </div>
    </main>
    {#if lab.panels.rightOpen}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="handle"
        title={t('circuit.bar.resize')}
        onpointerdown={(e) => resize('right', e)}
      ></div>
      <aside class="panel side" aria-label={t('circuit.bar.inspector')}><Inspector /></aside>
    {/if}
  </div>
  {#if lab.newPartOpen}<NewPart />{/if}
</div>

{#if lab.menu}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="scrim"
    onpointerdown={() => ((lab.menu = null), (lab.preview = null))}
    oncontextmenu={(e) => {
      e.preventDefault();
      lab.menu = null;
      lab.preview = null;
    }}
  ></div>
  {#key lab.menu}
    <CircuitMenu
      items={lab.menu.items}
      x={lab.menu.x}
      y={lab.menu.y}
      onclose={() => ((lab.menu = null), (lab.preview = null))}
    />
  {/key}
{/if}

<style>
  /* The mat's own colours (a green cutting mat in both themes) and the selection glow. */
  .circuit {
    --mat: oklch(0.43 0.05 158);
    --mat-major: oklch(0.93 0.03 150 / 0.24);
    --mat-minor: oklch(0.93 0.03 150 / 0.09);
    --select: #7ef0e6;
    position: relative;
    padding: var(--space-2) var(--space-3);
    background: var(--surface-2);
    outline: none;
    flex: 1;
    min-height: 0;
  }
  .circuit :global(.chip) {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 99px;
    font-variant-numeric: tabular-nums;
    border: 0;
  }
  .circuit :global(.chip.err) {
    background: var(--danger-soft);
    color: var(--danger-text);
  }
  .circuit :global(.chip.warn) {
    background: var(--warn-soft);
    color: var(--warn-text);
  }
  .circuit :global(.chip.ok) {
    background: var(--ok-soft);
    color: var(--ok-text);
  }
  .empty {
    position: absolute;
    inset: 0;
    margin: auto;
    width: min(360px, 80%);
    height: fit-content;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    text-align: center;
    padding: 18px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
  }
  .empty p {
    margin: 0;
    font-weight: 700;
  }
  .empty small {
    color: var(--text-muted);
  }
  .app {
    height: 100%;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 10px;
  }
  .work {
    display: grid;
    min-height: 0;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
  }
  .doc {
    font-family: var(--font-data);
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .spacer {
    flex: 1;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .icon {
    width: 32px;
    height: 32px;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    border-radius: var(--radius-md);
    display: grid;
    place-items: center;
    cursor: pointer;
    color: var(--text-muted);
  }
  .icon svg {
    width: 16px;
    height: 16px;
  }
  .btn.power svg {
    width: 14px;
    height: 14px;
  }
  .btn.power.on {
    background: #16a34a;
    border-color: #16a34a;
    color: #fff;
  }
  button.chip {
    border: 0;
    cursor: pointer;
  }
  .panel {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-pop);
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .handle {
    cursor: col-resize;
    position: relative;
    touch-action: none;
  }
  .handle::after {
    content: '';
    position: absolute;
    top: 40%;
    bottom: 40%;
    left: 3px;
    width: 2px;
    border-radius: 2px;
    background: var(--line);
  }
  .handle:hover::after {
    background: var(--accent);
    top: 20%;
    bottom: 20%;
  }
  .stage {
    position: relative;
    min-height: 0;
    min-width: 0;
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--mat);
    box-shadow: var(--shadow-pop);
  }
  .hint {
    position: absolute;
    left: 50%;
    top: 10px;
    transform: translateX(-50%);
    background: oklch(0.2 0.02 160 / 0.82);
    color: oklch(0.97 0.01 150);
    padding: 6px 12px;
    border-radius: 99px;
    font-size: 12.5px;
    max-width: calc(100% - 20px);
    text-align: center;
  }
  .powerbar {
    position: absolute;
    left: 10px;
    right: 10px;
    top: 48px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-start;
    pointer-events: none;
  }
  .powerbar span {
    padding: 5px 10px;
    border-radius: var(--radius-md);
    font-size: 12.5px;
    font-weight: 600;
    color: #fff;
    max-width: 100%;
  }
  .powerbar .ok {
    background: #15803d;
  }
  .powerbar .warn {
    background: #b45309;
  }
  .powerbar .error {
    background: #b91c1c;
  }
  .powerbar .motors {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 3px 4px 3px 10px;
    border-radius: var(--radius-md);
    background: oklch(0.2 0.02 160 / 0.7);
    color: oklch(0.97 0.01 150);
    font-size: 12px;
    font-weight: 600;
  }
  .powerbar .motors button {
    border: 0;
    background: transparent;
    color: inherit;
    font-size: 12px;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .powerbar .motors button.on {
    background: var(--surface);
    color: var(--text);
  }
  .powerbar small {
    color: oklch(0.97 0.01 150);
    font-size: 11.5px;
    opacity: 0.85;
  }
  .legend {
    position: absolute;
    left: 10px;
    bottom: 10px;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    max-width: calc(100% - 110px);
    background: oklch(0.2 0.02 160 / 0.55);
    color: oklch(0.97 0.01 150);
    padding: 6px 10px;
    border-radius: var(--radius-md);
    font-size: 12px;
    backdrop-filter: blur(4px);
  }
  .legend i {
    display: inline-block;
    width: 16px;
    height: 4px;
    border-radius: 2px;
    vertical-align: middle;
    margin-right: 5px;
  }
  .scale {
    font-family: var(--font-data);
    opacity: 0.85;
  }
  .zoom {
    position: absolute;
    right: 10px;
    bottom: 10px;
    display: flex;
    gap: 6px;
  }
  .zoom button {
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    border: 0;
    cursor: pointer;
    background: oklch(0.2 0.02 160 / 0.6);
    color: oklch(0.97 0.01 150);
    font-size: 16px;
    font-weight: 600;
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 45;
  }
  @media (max-width: 900px) {
    .app {
      height: auto;
    }
    .work {
      grid-template-columns: minmax(0, 1fr) !important;
      grid-template-rows: 62vh auto auto;
      gap: 10px;
    }
    .handle {
      display: none;
    }
    .stage {
      min-height: 360px;
      order: 1;
    }
    .side {
      order: 2;
      max-height: 70vh;
    }
    .lib {
      order: 3;
      max-height: 300px;
    }
    .scale {
      display: none;
    }
  }
</style>
