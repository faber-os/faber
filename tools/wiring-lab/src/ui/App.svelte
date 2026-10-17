<script lang="ts">
  /** The lab: parts on the left, the mat in the middle, the inspector on the right. */
  import Canvas from './Canvas.svelte';
  import ContextMenu from './ContextMenu.svelte';
  import Inspector from './Inspector.svelte';
  import Library from './Library.svelte';
  import NewPart from './NewPart.svelte';
  import { lab } from './lab.svelte';
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
    if (kinds.has('usb')) out.push(['#8b939c', 'USB']);
    out.push(['linear-gradient(90deg,#3b82f6,#facc15,#c084fc)', 'Signals']);
    return out;
  });

  onMount(() => {
    requestAnimationFrame(() => lab.fit());
  });

  function onkeydown(e: KeyboardEvent) {
    if ((e.target as Element).matches('input, textarea, select')) return;
    if (e.key === 'Escape') {
      if (lab.menu) lab.menu = null;
      else if (lab.connecting) {
        lab.connecting = null;
        lab.hint = '';
      } else if (lab.newPartOpen) lab.newPartOpen = false;
      else {
        lab.sel = null;
        lab.hl = null;
      }
    }
    if ((e.key === 'Delete' || e.key === 'Backspace') && lab.sel) {
      e.preventDefault();
      if ('part' in lab.sel) lab.removePart(lab.sel.part);
      else lab.removeWire(lab.sel.wire);
    }
    if ((e.key === 'r' || e.key === 'R') && lab.sel && 'part' in lab.sel && !e.ctrlKey && !e.metaKey) lab.rotate(lab.sel.part);
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      lab.undo();
    }
  }

  async function copy() {
    const text = JSON.stringify($state.snapshot(lab.doc), null, 2);
    try {
      await navigator.clipboard.writeText(text);
      lab.say('The drawing is copied as JSON.');
    } catch {
      lab.say('Copying was blocked. The drawing is saved in this browser anyway.');
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="app">
  <header class="bar">
    <h1>Wiring</h1>
    <span class="doc">{lab.doc.name} · {lab.doc.parts.length} parts · {lab.doc.wires.length} wires</span>
    {#if errors}<span class="chip err">{errors} problem{errors > 1 ? 's' : ''}</span>{:else if warns}<span class="chip warn">{warns} to look at</span>{:else}<span class="chip ok">Ready to build</span>{/if}
    <span class="spacer"></span>
    <div class="tools">
      <button class="btn primary" onclick={() => (lab.arrange(), requestAnimationFrame(() => lab.fit()))} title="Place every part and route the wires">Arrange</button>
      <button class="btn" onclick={() => lab.undo()} title="Undo (Ctrl+Z)">Undo</button>
      <button class="btn" onclick={() => lab.fit()}>Fit</button>
      <button class="btn" onclick={() => (lab.reset(), requestAnimationFrame(() => lab.fit()))} title="Back to the SENNEP example">Example</button>
      <button class="btn" onclick={copy}>Copy JSON</button>
    </div>
  </header>

  <aside class="panel lib" aria-label="Parts"><Library /></aside>

  <main class="stage">
    <Canvas />
    {#if lab.hint}<div class="hint">{lab.hint}</div>{/if}
    <div class="legend">
      {#each legend as [c, t] (t)}<span><i style:background={c}></i>{t}</span>{/each}
      <span class="scale">Big squares 1 in (25.4 mm)</span>
    </div>
    <div class="zoom">
      <button aria-label="Zoom out" onclick={() => { const r = document.getElementById('canvas')!.getBoundingClientRect(); lab.zoomAt(r.width / 2, r.height / 2, 0.8); }}>−</button>
      <button aria-label="Zoom in" onclick={() => { const r = document.getElementById('canvas')!.getBoundingClientRect(); lab.zoomAt(r.width / 2, r.height / 2, 1.25); }}>+</button>
    </div>
  </main>

  <aside class="panel side" aria-label="Inspector"><Inspector /></aside>
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
    grid-template-columns: 236px minmax(0, 1fr) 330px;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 12px;
  }
  .bar {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
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
  .panel {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow);
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .stage {
    position: relative;
    min-height: 0;
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
    background: oklch(0.2 0.02 160 / 0.78);
    color: oklch(0.97 0.01 150);
    padding: 6px 12px;
    border-radius: 99px;
    font-size: 12.5px;
    max-width: calc(100% - 20px);
    text-align: center;
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
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto 62vh auto auto;
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
