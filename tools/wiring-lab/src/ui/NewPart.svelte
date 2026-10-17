<script lang="ts">
  /** New part: a plain drawing with pins on its edges, or a picture of the real board on top. */
  import { stubOf } from '../lib/geometry';
  import { drawingToml, makeDef } from '../lib/newpart';
  import { lab } from './lab.svelte';
  import { G, art } from './looks';

  let name = $state('Line sensor (3 channel)');
  let widthMm = $state(40);
  let heightMm = $state(14);
  let color = $state('#1d4f9a');
  let pins = $state('VCC  bottom  vcc  3.3-5.5\nGND  bottom  gnd\nL    bottom  io  drives=5\nC    bottom  io  drives=5\nR    bottom  io  drives=5');
  let picture = $state<string | null>(null);
  let pictureError = $state('');

  const made = $derived(makeDef({ name, widthMm, heightMm, color, pins, art: picture }));
  const snippet = $derived(drawingToml(made.def));
  const pad = 40;

  function close() {
    lab.newPartOpen = false;
  }

  function add() {
    if (made.errors.length || !made.def.pins.length) return;
    lab.custom = [...lab.custom.filter((d) => d.id !== made.def.id), made.def];
    close();
    lab.addPart(made.def.id);
  }

  function onfile(e: Event) {
    const f = (e.currentTarget as HTMLInputElement).files?.[0];
    pictureError = '';
    if (!f) {
      picture = null;
      return;
    }
    if (f.size > 1.5e6) {
      pictureError = 'That picture is over 1.5 MB. Use a smaller one.';
      return;
    }
    const r = new FileReader();
    r.onload = () => (picture = r.result as string);
    r.readAsDataURL(f);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="back" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="modal" role="dialog" aria-modal="true" aria-labelledby="np-title">
    <h2 id="np-title">New part</h2>
    <div class="col">
      <label class="field">Name<input bind:value={name} /></label>
      <div class="row">
        <label class="field">Width (mm)<input type="number" min="5" max="300" bind:value={widthMm} /></label>
        <label class="field">Height (mm)<input type="number" min="5" max="300" bind:value={heightMm} /></label>
        <label class="field"
          >Board colour
          <select bind:value={color}>
            <option value="#1d4f9a">Blue PCB</option>
            <option value="#1f6e3d">Green PCB</option>
            <option value="#a3322b">Red PCB</option>
            <option value="#4b2f86">Purple PCB</option>
            <option value="#24272c">Black</option>
          </select>
        </label>
      </div>
      <label class="field">Pins, one per line: name, edge, kind, details<textarea spellcheck="false" bind:value={pins}></textarea></label>
      <p class="note">
        Edges: <code>top</code> <code>bottom</code> <code>left</code> <code>right</code>. Kinds: <code>vcc 3.3-5.5</code> (power in), <code>gnd</code>,
        <code>io</code>, <code>pwm</code>, <code>analog</code>, <code>sda</code>, <code>scl</code>, <code>supply 12.6</code> (power out), <code>motor</code>. Add
        <code>drives=5</code> for a pin that outputs 5 V logic, <code>optional</code> for one that may stay free. Pins sit 2.54 mm apart, centred on their edge.
      </p>
      <label class="field">Drawing (optional): an SVG or PNG of the top view<input type="file" accept="image/svg+xml,image/png,image/jpeg,image/webp" onchange={onfile} /></label>
      {#if made.errors.length || pictureError}<p class="err">{[...made.errors, pictureError].filter(Boolean).join(' ')}</p>{/if}
    </div>
    <div class="col">
      <div class="preview">
        <svg viewBox="{-pad} {-pad} {made.def.w * G + pad * 2} {made.def.h * G + pad * 2}" width={made.def.w * G + pad * 2}>
          {@html art(made.def)}
          {#each made.def.pins as p (p.id)}
            {@const cells = stubOf(made.def, p)}
            {@const end = cells[cells.length - 1] ?? [p.x, p.y]}
            <line x1={p.x * G} y1={p.y * G} x2={end[0] * G} y2={end[1] * G} stroke="#facc15" stroke-width="2.5" />
            <rect x={p.x * G - 3.4} y={p.y * G - 3.4} width="6.8" height="6.8" rx="1" fill="#e2bd52" stroke="#8f6f17" />
            <text
              x={p.x * G + (p.d === 1 ? -6 : p.d === 3 ? 6 : 0)}
              y={p.y * G + (p.d === 0 ? 6 : p.d === 2 ? -6 : 0)}
              class="pl"
              text-anchor={p.d === 0 ? 'end' : p.d === 2 ? 'start' : p.d === 1 ? 'end' : 'start'}
              dominant-baseline="central"
              transform={p.d === 0 || p.d === 2 ? `rotate(-90 ${p.x * G} ${p.y * G + (p.d === 0 ? 6 : -6)})` : undefined}>{p.label}</text
            >
          {/each}
        </svg>
      </div>
      <p class="note">What a contributor adds to the part's file (<code>parts/*.toml</code>):</p>
      <pre>{snippet}</pre>
    </div>
    <div class="actions">
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" disabled={!!made.errors.length || !made.def.pins.length} onclick={add}>Add to parts</button>
    </div>
  </div>
</div>

<style>
  .back {
    position: fixed;
    inset: 0;
    background: oklch(0.15 0.01 250 / 0.45);
    z-index: 40;
    display: grid;
    place-items: center;
    padding: 16px;
  }
  .modal {
    width: min(880px, 100%);
    max-height: 100%;
    overflow: auto;
    background: var(--surface);
    border-radius: var(--r-lg);
    border: 1px solid var(--line);
    box-shadow: var(--shadow-pop);
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 18px;
    padding: 18px;
  }
  h2 {
    grid-column: 1 / -1;
    font-size: 16px;
    margin: 0;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .row > .field {
    flex: 1;
    min-width: 90px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12.5px;
    color: var(--muted);
  }
  .field input,
  .field select,
  .field textarea {
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    padding: 6px 9px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 13.5px;
    width: 100%;
  }
  textarea {
    font-family: var(--font-data);
    font-size: 12.5px !important;
    min-height: 140px;
    resize: vertical;
    line-height: 1.5;
  }
  .note {
    font-size: 12.5px;
    color: var(--muted);
    margin: 0;
  }
  code,
  pre {
    font-family: var(--font-data);
    font-size: 12px;
  }
  pre {
    margin: 0;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 10px;
    overflow: auto;
    max-height: 240px;
    line-height: 1.5;
  }
  .preview {
    background: var(--mat);
    border-radius: var(--r-md);
    min-height: 200px;
    display: grid;
    place-items: center;
    padding: 10px;
  }
  .preview svg {
    max-width: 100%;
    max-height: 260px;
  }
  .pl {
    font-family: var(--font-data);
    font-size: 6.5px;
    fill: #f4f7f8;
  }
  .err {
    color: var(--danger);
    font-size: 12.5px;
    margin: 0;
  }
  .actions {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  @media (max-width: 760px) {
    .modal {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
