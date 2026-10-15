<script lang="ts">
  import { useCore } from '../lib/state/context';
  import type { ErrorBody } from '../lib/state/types';
  import { INTENSITY_BUCKETS, intensityRamp } from '../lib/viewers/colour';
  import { drawLaserScan, type ScanPalette } from '../lib/viewers/draw';
  import {
    scanStats,
    scanToPoints,
    type ScanInput,
    type ScanPoints,
    type ScanStats,
  } from '../lib/viewers/laserScan';
  import { fitView, panBy, zoomAt, zoomCentre, type View } from '../lib/viewers/view';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import { formatRate } from '../components/format';
  import Icon from '../components/Icon.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import { useTheme } from '../components/theme.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win, visible }: WindowProps = $props();

  const MAX_HZ = 15;
  const ZOOM_STEP = 1.4;
  const PAN_STEP = 40;
  const DEFAULT_RANGE_M = 5;

  const core = useCore();
  const theme = useTheme();

  const address = $derived(win.params.address ?? '');
  const partName = $derived(win.params.part ?? address.split('/')[0] ?? '');
  const part = $derived(core.parts?.find((p) => p.name === partName) ?? null);

  let box: HTMLDivElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let stats = $state<ScanStats | null>(null);
  let error = $state<ErrorBody | null>(null);
  let waiting = $state(true);

  // Drawing state lives outside runes: it changes every frame and nothing in the markup reads it.
  let latest: ScanInput | null = null;
  let fresh = false;
  let points: ScanPoints | null = null;
  let view: View | null = null;
  let size = { width: 0, height: 0 };
  let dpr = 1;
  let palette: ScanPalette | null = null;
  let paletteTheme = '';
  let dirty = true;

  $effect(() => {
    const stop = core.watchSignal(address, MAX_HZ, {
      onReading: (r) => {
        if (r.capability !== 'laser-scan@1') return;
        latest = r.reading;
        fresh = true;
        dirty = true;
        error = null;
      },
      onError: (e) => (error = e),
    });
    return stop;
  });

  // Size the canvas backing store to the box, in device pixels.
  $effect(() => {
    if (!box || !canvas) return;
    const c = canvas;
    const ro = new ResizeObserver(([entry]) => {
      if (!entry) return;
      dpr = window.devicePixelRatio || 1;
      size = { width: entry.contentRect.width, height: entry.contentRect.height };
      c.width = Math.max(1, Math.round(size.width * dpr));
      c.height = Math.max(1, Math.round(size.height * dpr));
      dirty = true;
    });
    ro.observe(box);
    return () => ro.disconnect();
  });

  // Draw only the newest reading, once per animation frame, and only while visible.
  $effect(() => {
    if (!visible || !canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) {
      error = {
        code: 'no-canvas',
        message: 'This browser could not start drawing the scan.',
        hint: 'Try another browser, or turn on hardware acceleration in the browser settings.',
      };
      return;
    }
    const resolvedTheme = theme.resolved;
    dirty = true;
    let raf = requestAnimationFrame(function frame() {
      raf = requestAnimationFrame(frame);
      if (paletteTheme !== resolvedTheme) {
        palette = readPalette(canvas!, resolvedTheme);
        paletteTheme = resolvedTheme;
        dirty = true;
      }
      if (fresh && latest) {
        fresh = false;
        points = scanToPoints(latest);
        stats = scanStats(latest);
        waiting = false;
        if (view === null && size.width > 0) view = fitView(farthest(points), size);
        dirty = true;
      }
      if (!dirty || size.width === 0 || !palette) return;
      dirty = false;
      view ??= fitView(DEFAULT_RANGE_M, size);
      drawLaserScan(ctx, points, view, size, dpr, palette);
    });
    return () => cancelAnimationFrame(raf);
  });

  // Wheel zoom needs a non-passive listener so the page itself does not scroll.
  $effect(() => {
    if (!canvas) return;
    const c = canvas;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = c.getBoundingClientRect();
      setView(
        zoomAt(
          currentView(),
          Math.exp(-e.deltaY * 0.0015),
          e.clientX - r.left,
          e.clientY - r.top,
          size,
        ),
      );
    };
    c.addEventListener('wheel', onWheel, { passive: false });
    return () => c.removeEventListener('wheel', onWheel);
  });

  function readPalette(el: HTMLElement, t: string): ScanPalette {
    const css = getComputedStyle(el);
    const v = (name: string) => css.getPropertyValue(name).trim();
    return {
      background: v('--scan-bg'),
      ring: v('--scan-ring'),
      axis: v('--scan-axis'),
      label: v('--scan-label'),
      origin: v('--scan-origin'),
      points: intensityRamp(INTENSITY_BUCKETS, t === 'dark' ? 'dark' : 'light'),
    };
  }

  /** Distance of the farthest point, for the first view. */
  function farthest(p: ScanPoints): number {
    let max = 0;
    for (let i = 0; i < p.count; i++) max = Math.max(max, Math.hypot(p.x[i]!, p.y[i]!));
    return max > 0 ? max : DEFAULT_RANGE_M;
  }

  function currentView(): View {
    return view ?? fitView(DEFAULT_RANGE_M, size);
  }

  function setView(v: View) {
    view = v;
    dirty = true;
  }

  function resetView() {
    setView(fitView(points && points.count > 0 ? farthest(points) : DEFAULT_RANGE_M, size));
  }

  // Pan with one pointer, pinch-zoom with two.
  const pointers = new Map<number, { x: number; y: number }>();

  function onPointerDown(e: PointerEvent) {
    canvas?.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
  }

  function onPointerMove(e: PointerEvent) {
    const prev = pointers.get(e.pointerId);
    if (!prev || !canvas) return;
    const next = { x: e.clientX, y: e.clientY };
    if (pointers.size === 1) {
      setView(panBy(currentView(), next.x - prev.x, next.y - prev.y));
    } else if (pointers.size === 2) {
      const other = [...pointers.entries()].find(([id]) => id !== e.pointerId)?.[1];
      if (other) {
        const r = canvas.getBoundingClientRect();
        const before = Math.hypot(prev.x - other.x, prev.y - other.y);
        const after = Math.hypot(next.x - other.x, next.y - other.y);
        const mx = (next.x + other.x) / 2 - r.left;
        const my = (next.y + other.y) / 2 - r.top;
        let v = currentView();
        if (before > 0) v = zoomAt(v, after / before, mx, my, size);
        v = panBy(v, (next.x - prev.x) / 2, (next.y - prev.y) / 2);
        setView(v);
      }
    }
    pointers.set(e.pointerId, next);
  }

  function onPointerUp(e: PointerEvent) {
    pointers.delete(e.pointerId);
  }

  function onKey(e: KeyboardEvent) {
    const v = currentView();
    const keys: Record<string, () => View> = {
      '+': () => zoomCentre(v, ZOOM_STEP, size),
      '=': () => zoomCentre(v, ZOOM_STEP, size),
      '-': () => zoomCentre(v, 1 / ZOOM_STEP, size),
      ArrowLeft: () => panBy(v, PAN_STEP, 0),
      ArrowRight: () => panBy(v, -PAN_STEP, 0),
      ArrowUp: () => panBy(v, 0, PAN_STEP),
      ArrowDown: () => panBy(v, 0, -PAN_STEP),
    };
    if (e.key === '0') {
      e.preventDefault();
      resetView();
    } else if (keys[e.key]) {
      e.preventDefault();
      setView(keys[e.key]!());
    }
  }
</script>

<div class="viewer">
  <div class="stage" bind:this={box}>
    <canvas
      bind:this={canvas}
      class="canvas"
      tabindex="0"
      aria-label="Top view of {address}, forward is up. Plus and minus zoom, arrow keys pan, 0 resets the view."
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      ondblclick={resetView}
      onkeydown={onKey}
    ></canvas>

    {#if error}
      <div class="overlay">
        <ErrorNotice {error} />
      </div>
    {:else if waiting}
      <div class="overlay quiet" role="status">
        <p>Waiting for the first scan from <code>{address}</code>…</p>
        {#if part && part.state.kind !== 'running'}
          <p class="sub">
            The part is {part.state.kind}{part.state.message ? `: ${part.state.message}` : '.'}
          </p>
        {/if}
      </div>
    {/if}

    <div class="zoom" role="group" aria-label="View">
      <button
        class="zbtn"
        aria-label="Zoom in"
        onclick={() => setView(zoomCentre(currentView(), ZOOM_STEP, size))}
      >
        <Icon name="zoomIn" />
      </button>
      <button
        class="zbtn"
        aria-label="Zoom out"
        onclick={() => setView(zoomCentre(currentView(), 1 / ZOOM_STEP, size))}
      >
        <Icon name="zoomOut" />
      </button>
      <button class="zbtn" aria-label="Fit the scan" onclick={resetView}>
        <Icon name="fit" />
      </button>
    </div>
  </div>

  <dl class="stats" aria-label="Scan statistics">
    <div>
      <dt>Points</dt>
      <dd class="num" data-testid="scan-points">{stats ? stats.points : '–'}</dd>
    </div>
    <div>
      <dt>No return</dt>
      <dd class="num" data-testid="scan-no-return">{stats ? stats.noReturn : '–'}</dd>
    </div>
    <div>
      <dt>Rate</dt>
      <dd class="num" data-testid="scan-rate">{formatRate(stats?.rateHz ?? null)}</dd>
    </div>
    <div class="grow">
      <dt class="visually-hidden">Signal</dt>
      <dd class="addr"><code>{address}</code></dd>
    </div>
    {#if part}
      <div>
        <dt class="visually-hidden">Part state</dt>
        <dd><StateBadge kind={part.state.kind} /></dd>
      </div>
    {/if}
  </dl>
</div>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--scan-bg);
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    touch-action: none;
    cursor: grab;
    outline: none;
  }
  .canvas:active {
    cursor: grabbing;
  }
  .canvas:focus-visible {
    box-shadow: inset 0 0 0 2px var(--focus-ring);
  }
  .overlay {
    position: absolute;
    left: var(--space-4);
    right: var(--space-4);
    top: var(--space-4);
    max-width: 440px;
    pointer-events: none;
  }
  .overlay :global(button) {
    pointer-events: auto;
  }
  .overlay.quiet {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
    font-size: var(--text-sm);
  }
  .sub {
    color: var(--text-muted);
  }
  .zoom {
    position: absolute;
    right: var(--space-3);
    bottom: var(--space-3);
    display: flex;
    flex-direction: column;
    padding: 3px;
    border-radius: var(--radius-md);
    background: var(--chrome);
    box-shadow: var(--shadow-pop);
  }
  .zbtn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--touch);
    height: var(--touch);
    border: 0;
    border-radius: calc(var(--radius-md) - 3px);
    background: transparent;
    color: var(--text);
  }
  .zbtn:hover {
    background: var(--hover);
  }
  .stats {
    display: flex;
    align-items: center;
    gap: var(--space-5);
    margin: 0;
    padding: var(--space-2) var(--space-4);
    min-height: 44px;
    border-top: 1px solid var(--line);
    background: var(--surface-2);
    font-size: var(--text-sm);
  }
  .stats div {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
  }
  .stats .grow {
    flex: 1;
    min-width: 0;
    justify-content: flex-end;
  }
  dt {
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    font-weight: 650;
  }
  .addr {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 400;
    color: var(--text-muted);
  }
  @media (max-width: 520px) {
    .stats {
      gap: var(--space-3);
    }
    .stats .grow {
      display: none;
    }
  }
</style>
