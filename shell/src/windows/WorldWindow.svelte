<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { WorldUpdate } from '../lib/state/core.svelte';
  import type {
    DistanceReading,
    ErrorBody,
    LaserScanReading,
    WorldInfo,
    WorldView,
  } from '../lib/state/types';
  import { worldBase, worldDistances, worldParts, worldScanners } from '../lib/state/world';
  import { scanToPoints, type ScanPoints } from '../lib/viewers/laserScan';
  import { formatLength, formatSpeed } from '../lib/viewers/units';
  import {
    buildStatic,
    drawWorld,
    machineOnScreen,
    type StaticLayer,
    type WorldPalette,
    type WorldScene,
  } from '../lib/viewers/worldDraw';
  import {
    Trail,
    centreOn,
    coneLength,
    conePolygon,
    coneTone,
    fitBounds,
    mountPose,
    panBy,
    pointsToWorld,
    zoomAt,
    type Camera,
    type Pose2,
  } from '../lib/viewers/worldView';
  import Dialog from '../components/Dialog.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import RichText from '../components/RichText.svelte';
  import { openDrive } from '../components/driveWindow';
  import { useDesktop } from '../components/desktopController.svelte';
  import { useTheme } from '../components/theme.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { visible }: WindowProps = $props();

  /** Sensors are drawn at most this often; the world itself comes 20 times a second. */
  const SENSOR_HZ = 10;
  const TRAIL_US = 5_000_000;
  const ZOOM_STEP = 1.4;
  /** Following the machine shows at least this much around it (m), across the smaller side. */
  const FOLLOW_SPAN_M = 12;

  const core = useCore();
  const desktop = useDesktop();
  const theme = useTheme();

  const parts = $derived(core.parts);
  const inWorld = $derived(worldParts(parts));
  const base = $derived(worldBase(parts));
  const scanners = $derived(worldScanners(parts));
  const distances = $derived(worldDistances(parts));
  const unit = $derived(core.settings?.units.length ?? 'metric');

  let view = $state.raw<WorldView | null>(null);
  let worlds = $state.raw<WorldInfo[] | null>(null);
  let error = $state.raw<ErrorBody | null>(null);
  let picking = $state(false);
  let switchTo = $state.raw<WorldInfo | null>(null);
  let switching = $state(false);
  let follow = $state(true);
  let bumped = $state(false);
  /** What the status bar shows, refreshed a few times a second. */
  let shown = $state.raw<{ x: number; y: number; speed: number } | null>(null);

  let box: HTMLDivElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let pickerBox: HTMLDivElement | undefined = $state();

  // Drawing state lives outside runes: it changes every frame and the markup doesn't read it.
  let latest: WorldUpdate | null = null;
  const trail = new Trail(TRAIL_US);
  const scans = new Map<string, { reading: LaserScanReading; points: ScanPoints }>();
  const ranges = new Map<string, DistanceReading>();
  let layer: StaticLayer<Path2D> | null = null;
  let cam: Camera | null = null;
  let size = { width: 0, height: 0 };
  let dpr = 1;
  let palette: WorldPalette | null = null;
  let paletteTheme = '';
  let raf = 0;
  let lastShown = 0;
  let draw: () => void = () => {};

  /** Something changed: draw it on the next animation frame (once, however much changed). */
  function invalidate() {
    if (raf === 0 && visible) raf = requestAnimationFrame(() => draw());
  }

  async function loadView() {
    const r = await core.loadWorld();
    if (!r.ok) {
      error = r.error;
      return;
    }
    error = null;
    view = r.value;
  }

  async function loadWorlds() {
    const r = await core.loadWorlds();
    if (r.ok) worlds = r.value;
    else error = r.error;
  }

  // The world's geometry: now, and again whenever the world is switched (here or elsewhere).
  $effect(() => {
    void loadView();
    void loadWorlds();
  });

  // A new world: its fixed geometry becomes paths once, and the view starts over.
  $effect(() => {
    const v = view;
    if (!v) return;
    untrack(() => {
      layer = buildStatic(v.geometry, () => new Path2D());
      // Where the machine was and what the sensors saw belong to the old world.
      latest = null;
      shown = null;
      trail.clear();
      scans.clear();
      ranges.clear();
      cam = null;
      invalidate();
    });
  });

  // The machine, 20 times a second, while the window shows.
  $effect(() => {
    if (!visible) return;
    return core.watchWorld({
      onState: (s) => {
        latest = s;
        trail.push(s.tUs, s.pose);
        if (s.bumped !== bumped) bumped = s.bumped;
        const now = performance.now();
        if (now - lastShown > 200) {
          lastShown = now;
          shown = { x: s.pose.x, y: s.pose.y, speed: Math.hypot(s.velocity.vx, s.velocity.vy) };
        }
        invalidate();
      },
      onChanged: () => void loadView(),
      onError: (e) => (error = e),
    });
  });

  // Every world LiDAR and distance sensor, while the window shows.
  $effect(() => {
    if (!visible) return;
    const stops = [
      ...scanners.map((s) =>
        core.watchSignal(s.address, SENSOR_HZ, {
          onReading: (r) => {
            if (r.capability !== 'laser-scan@1') return;
            scans.set(s.address, { reading: r.reading, points: scanToPoints(r.reading) });
            invalidate();
          },
          onError: () => scans.delete(s.address),
        }),
      ),
      ...distances.map((d) =>
        core.watchSignal(d.address, SENSOR_HZ, {
          onReading: (r) => {
            if (r.capability !== 'distance@1') return;
            ranges.set(d.address, r.reading);
            invalidate();
          },
          onError: () => ranges.delete(d.address),
        }),
      ),
    ];
    return () => {
      for (const stop of stops) stop();
      scans.clear();
      ranges.clear();
    };
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
      invalidate();
    });
    ro.observe(box);
    return () => ro.disconnect();
  });

  // The frame itself. Redraws only when invalidate() asked for it.
  $effect(() => {
    if (!visible || !canvas) return;
    const c = canvas;
    const ctx = c.getContext('2d');
    if (!ctx) {
      error = {
        code: 'no-canvas',
        message: t('world.canvas.error.message'),
        hint: t('world.canvas.error.hint'),
      };
      return;
    }
    const resolvedTheme = theme.resolved;
    const length = unit;
    const footprint = view?.footprint ?? null;
    const following = follow;
    const cones = distances;
    const lidars = scanners;
    draw = () => {
      raf = 0;
      if (size.width === 0 || !view) return;
      if (paletteTheme !== resolvedTheme || !palette) {
        palette = readPalette(c);
        paletteTheme = resolvedTheme;
      }
      const pose: Pose2 = latest?.pose ?? view.start;
      cam ??= firstCamera(pose);
      if (following) cam = centreOn(cam, pose.x, pose.y);
      const frame = scene(pose, footprint, cones, lidars);
      drawWorld(ctx, layer, frame, cam, size, dpr, palette, length);
      // Test hooks: where the machine is drawn, and how many laser points.
      c.dataset.scanPoints = String(frame.scans.reduce((n, s) => n + s.x.length, 0));
      const at = machineOnScreen(pose, cam, size);
      c.dataset.machineX = at ? String(Math.round(at.x)) : '';
      c.dataset.machineY = at ? String(Math.round(at.y)) : '';
    };
    invalidate();
    return () => {
      cancelAnimationFrame(raf);
      raf = 0;
      draw = () => {};
    };
  });

  function scene(
    pose: Pose2,
    footprint: WorldScene['footprint'],
    cones: typeof distances,
    lidars: typeof scanners,
  ): WorldScene {
    return {
      pose,
      footprint,
      bumped: latest?.bumped ?? false,
      movers: latest?.movers ?? [],
      trail: trail.poses,
      // Each scan where the machine was when it was taken, so a turn doesn't smear it.
      scans: lidars.flatMap((s) => {
        const scan = scans.get(s.address);
        if (!scan) return [];
        const at = trail.poseAt(scan.reading.tUs) ?? pose;
        const p = scan.points;
        return [pointsToWorld(mountPose(at, s.mount), p.x, p.y, p.count)];
      }),
      cones: cones.map((d) => {
        const reading = ranges.get(d.address) ?? null;
        const sensor = mountPose(pose, d.mount);
        return {
          polygon: conePolygon(sensor, d.info.fieldOfViewRad, coneLength(reading, d.info)),
          tone: coneTone(reading?.status ?? null),
        };
      }),
    };
  }

  function readPalette(el: HTMLElement): WorldPalette {
    const css = getComputedStyle(el);
    const v = (name: string) => css.getPropertyValue(name).trim();
    return {
      background: v('--scan-bg'),
      floor: v('--surface'),
      grid: v('--scan-ring'),
      wall: v('--text'),
      shape: v('--text-faint'),
      overhead: v('--text-faint'),
      label: v('--scan-label'),
      machine: v('--accent'),
      heading: v('--accent-text'),
      trail: v('--accent'),
      mover: v('--app-files'),
      scan: v('--info'),
      bumped: v('--danger'),
      cone: { ok: v('--ok'), near: v('--warn'), none: v('--text-faint'), bad: v('--danger') },
    };
  }

  function fitted(): Camera {
    return view ? fitBounds(view.geometry.bounds, size) : { scale: 50, cx: 0, cy: 0 };
  }

  function firstCamera(pose: Pose2): Camera {
    const fit = fitted();
    if (!follow) return fit;
    const span = Math.min(size.width, size.height) / FOLLOW_SPAN_M;
    return { scale: Math.max(fit.scale, span), cx: pose.x, cy: pose.y };
  }

  function current(): Camera {
    cam ??= fitted();
    return cam;
  }

  function setCamera(c: Camera) {
    cam = c;
    invalidate();
  }

  function zoomBy(factor: number) {
    setCamera(zoomAt(current(), factor, size.width / 2, size.height / 2, size));
  }

  function fitWorld() {
    follow = false;
    setCamera(fitted());
  }

  // Wheel zoom needs a non-passive listener so the page itself does not scroll.
  $effect(() => {
    if (!canvas) return;
    const c = canvas;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = c.getBoundingClientRect();
      const factor = Math.exp(-e.deltaY * 0.0015);
      // Following keeps the machine in the middle, so zoom there; else zoom at the pointer.
      const [sx, sy] = follow
        ? [size.width / 2, size.height / 2]
        : [e.clientX - r.left, e.clientY - r.top];
      setCamera(zoomAt(current(), factor, sx, sy, size));
    };
    c.addEventListener('wheel', onWheel, { passive: false });
    return () => c.removeEventListener('wheel', onWheel);
  });

  // Pan with one pointer (which stops following), pinch-zoom with two.
  const pointers = new Map<number, { x: number; y: number }>();

  function onPointerDown(e: PointerEvent) {
    canvas?.setPointerCapture?.(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
  }

  function onPointerMove(e: PointerEvent) {
    const prev = pointers.get(e.pointerId);
    if (!prev || !canvas) return;
    const next = { x: e.clientX, y: e.clientY };
    if (pointers.size === 1) {
      if (next.x !== prev.x || next.y !== prev.y) follow = false;
      setCamera(panBy(current(), next.x - prev.x, next.y - prev.y));
    } else if (pointers.size === 2) {
      const other = [...pointers.entries()].find(([id]) => id !== e.pointerId)?.[1];
      if (other) {
        const r = canvas.getBoundingClientRect();
        const before = Math.hypot(prev.x - other.x, prev.y - other.y);
        const after = Math.hypot(next.x - other.x, next.y - other.y);
        const mx = (next.x + other.x) / 2 - r.left;
        const my = (next.y + other.y) / 2 - r.top;
        if (before > 0) setCamera(zoomAt(current(), after / before, mx, my, size));
      }
    }
    pointers.set(e.pointerId, next);
  }

  function onPointerUp(e: PointerEvent) {
    pointers.delete(e.pointerId);
  }

  /** Ctrl +, Ctrl − and Ctrl 0 (fit the world) anywhere in the window, as in Files; +/− too. */
  function onKey(e: KeyboardEvent) {
    if (e.altKey || (e.target as HTMLElement).closest('input, select, textarea')) return;
    if (e.key === '+' || e.key === '=' || e.key === 'Add') zoomBy(ZOOM_STEP);
    else if (e.key === '-' || e.key === '_' || e.key === 'Subtract') zoomBy(1 / ZOOM_STEP);
    else if (e.key === '0' && (e.ctrlKey || e.metaKey)) fitWorld();
    else return;
    e.preventDefault();
  }

  function choose(w: WorldInfo) {
    picking = false;
    if (w.name !== view?.name) switchTo = w;
  }

  async function confirmSwitch() {
    const w = switchTo;
    if (!w) return;
    switching = true;
    const r = await core.setWorld(w.name);
    switching = false;
    switchTo = null;
    if (!r.ok) {
      error = r.error;
      return;
    }
    error = null;
    follow = true;
    view = r.value;
  }

  async function resetPosition() {
    const r = await core.resetWorld();
    if (!r.ok) {
      error = r.error;
      return;
    }
    trail.clear();
    follow = true;
    invalidate();
  }

  /** Opens Add part with Simulated → In the world and the generic drive base picked. */
  function addBase() {
    const id = desktop.open('parts');
    desktop.update(id, { params: { addSimulate: 'world', addConnector: 'generic-planar-base' } });
  }

  function sizeText(w: WorldInfo): string {
    return t('world.picker.size', {
      width: formatLength(w.size[0], unit),
      height: formatLength(w.size[1], unit),
    });
  }

  // Close the world list on a click elsewhere.
  $effect(() => {
    if (!picking) return;
    const close = (e: PointerEvent) => {
      if (pickerBox && !pickerBox.contains(e.target as Node)) picking = false;
    };
    window.addEventListener('pointerdown', close, true);
    return () => window.removeEventListener('pointerdown', close, true);
  });

  // Following again (or the world changed): bring the machine back to the middle.
  $effect(() => {
    if (follow) untrack(() => invalidate());
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="world" onkeydown={onKey}>
  <div class="toolbar">
    <div class="picker" bind:this={pickerBox}>
      <button
        class="btn"
        aria-haspopup="listbox"
        aria-expanded={picking}
        data-own-keys
        onclick={() => (picking = !picking)}
        onkeydown={(e) => e.key === 'Escape' && (picking = false)}
      >
        <Icon name="world" size={17} />
        <span class="title">{t('world.picker.button', { world: view?.title ?? '…' })}</span>
        <Icon name="down" size={15} />
      </button>
      {#if picking && worlds}
        <div class="list" role="listbox" aria-label={t('world.picker.label')} tabindex="-1">
          {#each worlds as w (w.name)}
            <button
              class="item"
              role="option"
              aria-selected={w.name === view?.name}
              onclick={() => choose(w)}
              onkeydown={(e) => e.key === 'Escape' && (picking = false)}
            >
              <span class="item-head">
                <span class="item-title">{w.title}</span>
                <span class="item-size num">{sizeText(w)}</span>
              </span>
              <span class="item-desc">{w.description}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <span class="grow"></span>
    <label class="follow" title={t('world.follow')}>
      <input
        class="switch"
        type="checkbox"
        role="switch"
        aria-label={t('world.follow')}
        bind:checked={follow}
      />
      <span class="follow-text">{t('world.follow')}</span>
    </label>
    <button class="btn" title={t('world.reset.title')} onclick={resetPosition}>
      <Icon name="undo" size={17} />
      <span>{t('world.reset')}</span>
    </button>
    {#if base}
      <button
        class="btn"
        title={t('world.drive.title', { part: base.name })}
        onclick={() => openDrive(desktop, base.name)}
      >
        <Icon name="drive" size={17} />
        <span>{t('world.drive')}</span>
      </button>
    {/if}
  </div>

  <div class="stage" bind:this={box}>
    <canvas
      bind:this={canvas}
      class="canvas"
      tabindex="0"
      data-testid="world-canvas"
      aria-label={t('world.canvas.label', { world: view?.title ?? '' })}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      ondblclick={fitWorld}
    ></canvas>

    {#if bumped}
      <div class="bumped" role="status" data-testid="world-bumped">
        <Icon name="warning" size={18} />
        <span><strong>{t('world.bumped')}</strong> {t('world.bumped.detail')}</span>
      </div>
    {/if}

    {#if error}
      <div class="overlay">
        <ErrorNotice {error} />
      </div>
    {:else if parts !== null && inWorld.length === 0}
      <div class="overlay card empty" data-testid="world-empty">
        <h3>{t('world.empty.title')}</h3>
        <p><RichText text={t('world.empty.body')} /></p>
        <button class="btn primary" onclick={addBase}>
          <Icon name="plus" size={17} />
          {t('world.empty.add')}
        </button>
      </div>
    {:else if !view}
      <div class="overlay card quiet" role="status">{t('world.loading')}</div>
    {/if}

    <div class="zoom" role="group" aria-label={t('world.zoom.label')}>
      <button class="zbtn" aria-label={t('world.zoom.in')} onclick={() => zoomBy(ZOOM_STEP)}>
        <Icon name="zoomIn" />
      </button>
      <button class="zbtn" aria-label={t('world.zoom.out')} onclick={() => zoomBy(1 / ZOOM_STEP)}>
        <Icon name="zoomOut" />
      </button>
      <button class="zbtn" aria-label={t('world.zoom.fit')} onclick={fitWorld}>
        <Icon name="fit" />
      </button>
    </div>
  </div>

  <dl class="stats">
    <div>
      <dt>{t('world.position')}</dt>
      <dd
        class="num"
        data-testid="world-position"
        data-x={shown?.x.toFixed(3)}
        data-y={shown?.y.toFixed(3)}
      >
        {shown
          ? t('world.position.value', {
              x: formatLength(shown.x, unit),
              y: formatLength(shown.y, unit),
            })
          : '–'}
      </dd>
    </div>
    <div>
      <dt>{t('world.speed')}</dt>
      <dd class="num">{shown ? formatSpeed(shown.speed, unit) : '–'}</dd>
    </div>
    {#if !base && inWorld.length > 0}
      <div class="grow"><dd class="muted">{t('world.noBase')}</dd></div>
    {/if}
  </dl>
</div>

{#if switchTo}
  <Dialog
    title={t('world.switch.title', { world: switchTo.title })}
    oncancel={() => (switchTo = null)}
  >
    <p>{t('world.switch.body')}</p>
    <p class="muted">{switchTo.description}</p>
    {#snippet actions()}
      <button class="btn" onclick={() => (switchTo = null)}>{t('world.switch.cancel')}</button>
      <button class="btn primary" disabled={switching} onclick={confirmSwitch}>
        {t('world.switch.confirm')}
      </button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .world {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--scan-bg);
  }
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .toolbar .btn {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .grow {
    flex: 1;
  }
  .picker {
    position: relative;
    min-width: 0;
  }
  .picker .title {
    font-weight: 650;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .list {
    position: absolute;
    z-index: 5;
    top: calc(100% + 4px);
    left: 0;
    display: grid;
    width: min(26rem, 80vw);
    max-height: 22rem;
    overflow-y: auto;
    padding: 4px;
    border-radius: var(--radius-md);
    background: var(--menu);
    box-shadow: var(--shadow-pop);
  }
  .item {
    display: grid;
    gap: 2px;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: calc(var(--radius-md) - 3px);
    background: transparent;
    color: var(--text);
    text-align: left;
  }
  .item:hover,
  .item:focus-visible {
    background: var(--hover);
  }
  .item[aria-selected='true'] {
    background: var(--selected);
  }
  .item-head {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
  }
  .item-title {
    font-weight: 650;
  }
  .item-size {
    font-size: var(--text-sm);
    color: var(--text-muted);
    white-space: nowrap;
  }
  .item-desc {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .follow {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    min-height: var(--touch);
    cursor: pointer;
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
  .bumped {
    position: absolute;
    top: var(--space-3);
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    max-width: calc(100% - 2 * var(--space-4));
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--danger);
    color: var(--surface);
    font-size: var(--text-sm);
    box-shadow: var(--shadow-pop);
    pointer-events: none;
  }
  .overlay {
    position: absolute;
    left: var(--space-4);
    right: var(--space-4);
    top: var(--space-4);
    max-width: 460px;
  }
  .card {
    display: grid;
    justify-items: start;
    gap: var(--space-2);
    padding: var(--space-4);
    border-radius: var(--radius-md);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
  }
  .card h3 {
    font-size: var(--text-lg);
    font-weight: 680;
  }
  .card p {
    color: var(--text-muted);
  }
  .card .btn {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    margin-top: var(--space-1);
  }
  .quiet {
    font-size: var(--text-sm);
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
    min-width: 0;
  }
  dt {
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    font-weight: 650;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .muted {
    color: var(--text-muted);
    font-weight: 400;
  }
  @media (max-width: 560px) {
    .follow-text {
      display: none;
    }
    .stats {
      gap: var(--space-3);
    }
  }
</style>
