<script lang="ts">
  /** The lab: parts on the left, the mat in the middle, the inspector on the right. */
  import Canvas from './Canvas.svelte';
  import ContextMenu from './ContextMenu.svelte';
  import Inspector from './Inspector.svelte';
  import Library from './Library.svelte';
  import NewPart from './NewPart.svelte';
  import { lab } from './lab.svelte';
  import { openMenu } from './menu';
  import { onMount } from 'svelte';

  const errors = $derived(lab.issues.filter((i) => i.sev === 'error').length);
  const warns = $derived(lab.issues.length - errors);
  const legend = $derived.by(() => {
    const kinds = new Set(lab.nets.nets.map((n) => (n.kind === 'power' ? (n.volts > 6 ? 'bat' : n.volts < 4 ? '3v3' : '5v') : n.kind)));
    const out: [string, string][] = [];
    if (kinds.has('bat')) out.push(['#be123c', 'Battery']);
    if (kinds.has('5v')) out.push(['#ef4444', '5 V']);
    if (kinds.has('3v3')) out.push(['#f97316', '3.3 V']);
    if (kinds.has('gnd')) out.push(['#202328', 'GND']);
    if (kinds.has('cable')) out.push(['#8b939c', 'Cables']);
    out.push(['linear-gradient(90deg,#3b82f6,#facc15,#c084fc)', 'Signals']);
    return out;
  });

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
      if (k === 'z' && !e.shiftKey) (take(), lab.undo());
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
      const [dx, dy] = ({ ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] } as Record<string, [number, number]>)[e.key]!;
      lab.nudge(dx, dy);
    }
  }

  async function copyJson() {
    const text = JSON.stringify($state.snapshot(lab.doc), null, 2);
    try {
      await navigator.clipboard.writeText(text);
      lab.say('The drawing is copied as JSON.');
    } catch {
      lab.say('Copying was blocked. The drawing is saved in this browser anyway.');
    }
  }

  // Dragging a panel's edge.
  function resize(side: 'left' | 'right', e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const x0 = e.clientX;
    const w0 = lab.panels[side];
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - x0;
      lab.panels[side] = Math.round(Math.min(560, Math.max(180, side === 'left' ? w0 + dx : w0 - dx)));
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
    const r = document.getElementById('canvas')!.getBoundingClientRect();
    lab.zoomAt(r.width / 2, r.height / 2, f);
  };
</script>

<svelte:window {onkeydown} />

<div class="app">
  <header class="bar">
    <button class="icon" title={lab.panels.leftOpen ? 'Hide the parts list' : 'Show the parts list'} aria-label="Toggle parts list" onclick={() => (lab.panels.leftOpen = !lab.panels.leftOpen)}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="2.5" width="13" height="11" rx="2" fill="none" stroke="currentColor" /><rect x="2" y="3" width="4" height="10" fill={lab.panels.leftOpen ? 'currentColor' : 'none'} /></svg>
    </button>
    <h1>Wiring</h1>
    <span class="doc">{lab.doc.name} · {lab.doc.parts.length} parts · {lab.doc.wires.length} wires</span>
    {#if errors}<button class="chip err" onclick={() => (lab.tab = 'checks')}>{errors} problem{errors > 1 ? 's' : ''}</button>{:else if warns}<button class="chip warn" onclick={() => (lab.tab = 'checks')}>{warns} to look at</button>{:else}<span class="chip ok">Ready to build</span>{/if}
    <span class="spacer"></span>
    <div class="tools">
      <button class="btn power" class:on={lab.power} aria-pressed={lab.power} title="Switch the drawing on (P)" onclick={() => (lab.power = !lab.power)}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 1.5v6M4.2 3.8a5.5 5.5 0 1 0 7.6 0" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
        {lab.power ? 'Power on' : 'Power off'}
      </button>
      <button class="btn" onclick={() => (lab.arrange(), requestAnimationFrame(() => lab.fit()))} title="Place every part and route the wires">Arrange</button>
      <button class="btn" onclick={() => lab.undo()} title="Undo (Ctrl+Z)">Undo</button>
      <button class="btn" onclick={() => lab.redo()} title="Redo (Ctrl+Y)">Redo</button>
      <button class="btn" onclick={() => lab.fit()} title="Fit (F)">Fit</button>
      <button class="btn" onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); openMenu(r.left, r.bottom + 4, [{ label: 'SENNEP V2, the whole machine', hint: 'From its electronics plan', action: () => (lab.reset('machine'), requestAnimationFrame(() => lab.fit())) }, { label: 'One corner of SENNEP', hint: 'Small: battery to one wheel', action: () => (lab.reset('corner'), requestAnimationFrame(() => lab.fit())) }]); }} title="Load an example">Example ▾</button>
      <button class="btn" onclick={copyJson}>Copy JSON</button>
    </div>
    <button class="icon" title={lab.panels.rightOpen ? 'Hide the inspector' : 'Show the inspector'} aria-label="Toggle inspector" onclick={() => (lab.panels.rightOpen = !lab.panels.rightOpen)}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="2.5" width="13" height="11" rx="2" fill="none" stroke="currentColor" /><rect x="10" y="3" width="4" height="10" fill={lab.panels.rightOpen ? 'currentColor' : 'none'} /></svg>
    </button>
  </header>

  <div class="work" style:grid-template-columns={cols}>
    {#if lab.panels.leftOpen}
      <aside class="panel lib" aria-label="Parts"><Library /></aside>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="handle" title="Drag to resize" onpointerdown={(e) => resize('left', e)}></div>
    {/if}
    <main class="stage">
      <Canvas />
      {#if lab.hint}<div class="hint">{lab.hint}</div>{/if}
      {#if lab.sim}
        <div class="powerbar">
          {#each lab.sim.messages.slice(0, 4) as m, i (i)}<span class={m.sev}>{m.text}</span>{/each}
          <div class="motors" role="radiogroup" aria-label="Motors">
            Motors
            {#each [['off', 'off'], ['run', 'driving'], ['stall', 'held still']] as [v, t] (v)}
              <button role="radio" aria-checked={lab.motors === v} class:on={lab.motors === v} onclick={() => (lab.motors = v as 'off' | 'run' | 'stall')}>{t}</button>
            {/each}
          </div>
          <small>Click a switch or the e-stop to flick it. Checks lists everything the power shows up. P turns power off.</small>
        </div>
      {/if}
      <div class="legend">
        {#each legend as [c, t] (t)}<span><i style:background={c}></i>{t}</span>{/each}
        <span class="scale">Big squares 1 in (25.4 mm)</span>
      </div>
      <div class="zoom">
        <button aria-label="Zoom out" onclick={() => zoom(0.8)}>−</button>
        <button aria-label="Zoom in" onclick={() => zoom(1.25)}>+</button>
      </div>
    </main>
    {#if lab.panels.rightOpen}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="handle" title="Drag to resize" onpointerdown={(e) => resize('right', e)}></div>
      <aside class="panel side" aria-label="Inspector"><Inspector /></aside>
    {/if}
  </div>
</div>

{#if lab.menu}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onpointerdown={() => ((lab.menu = null), (lab.preview = null))} oncontextmenu={(e) => { e.preventDefault(); lab.menu = null; lab.preview = null; }}></div>
  {#key lab.menu}
    <ContextMenu items={lab.menu.items} x={lab.menu.x} y={lab.menu.y} onclose={() => ((lab.menu = null), (lab.preview = null))} />
  {/key}
{/if}
{#if lab.newPartOpen}<NewPart />{/if}

<style>
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
  h1 {
    font-size: 17px;
    font-weight: 700;
    margin: 0;
  }
  .doc {
    font-family: var(--font-data);
    font-size: 12.5px;
    color: var(--muted);
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
    border-radius: var(--r-md);
    display: grid;
    place-items: center;
    cursor: pointer;
    color: var(--muted);
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
    border-radius: var(--r-lg);
    box-shadow: var(--shadow);
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
    border-radius: var(--r-lg);
    overflow: hidden;
    background: var(--mat);
    box-shadow: var(--shadow);
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
    border-radius: var(--r-md);
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
    border-radius: var(--r-md);
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
    border-radius: var(--r-sm);
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
    border-radius: var(--r-md);
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
    border-radius: var(--r-md);
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
