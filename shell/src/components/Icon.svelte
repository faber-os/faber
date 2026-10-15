<script lang="ts" module>
  // One icon family: 24-unit grid, 1.75 stroke, round caps. Drawn here so the shell works offline.
  const PATHS = {
    minimise: 'M6 17h12',
    maximise: 'M6.5 6.5h11v11h-11z',
    restore: 'M9 6.5h8.5V15M6.5 9h8.5v8.5H6.5z',
    close: 'M7 7l10 10M17 7L7 17',
    plus: 'M12 5.5v13M5.5 12h13',
    back: 'M14.5 6.5L9 12l5.5 5.5',
    trash: 'M5.5 7.5h13M10 7.5V5.5h4v2M7.5 7.5l.8 11h7.4l.8-11',
    scan: 'M12 12m-7.5 0a7.5 7.5 0 1 0 15 0a7.5 7.5 0 1 0-15 0M12 12m-3.5 0a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0-7 0M12 12l5.3-5.3',
    parts:
      'M8 8h8v8H8zM10 4.5V8M14 4.5V8M10 16v3.5M14 16v3.5M4.5 10H8M4.5 14H8M16 10h3.5M16 14h3.5',
    serial: 'M4.5 6.5h15v11h-15zM8 10.5l2.5 2-2.5 2M12.5 14.5H16',
    logs: 'M8.5 7.5h10M8.5 12h10M8.5 16.5h10M5.5 7.5h.01M5.5 12h.01M5.5 16.5h.01',
    info: 'M12 12m-8 0a8 8 0 1 0 16 0a8 8 0 1 0-16 0M12 11v5M12 8h.01',
    zoomIn: 'M10.5 10.5m-6 0a6 6 0 1 0 12 0a6 6 0 1 0-12 0M15 15l4.5 4.5M10.5 8v5M8 10.5h5',
    zoomOut: 'M10.5 10.5m-6 0a6 6 0 1 0 12 0a6 6 0 1 0-12 0M15 15l4.5 4.5M8 10.5h5',
    fit: 'M4.5 9V4.5H9M15 4.5h4.5V9M19.5 15v4.5H15M9 19.5H4.5V15M12 12h.01',
    sun: 'M12 12m-3.5 0a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0-7 0M12 3.5v2M12 18.5v2M3.5 12h2M18.5 12h2M6 6l1.4 1.4M16.6 16.6L18 18M6 18l1.4-1.4M16.6 7.4L18 6',
    moon: 'M18.5 14.5A7 7 0 0 1 9.5 5.5a7 7 0 1 0 9 9z',
    auto: 'M12 12m-7.5 0a7.5 7.5 0 1 0 15 0a7.5 7.5 0 1 0-15 0M12 4.5v15',
    send: 'M5 12h12.5M12.5 7l5 5-5 5',
    chevron: 'M9.5 6.5L15 12l-5.5 5.5',
    clear: 'M5.5 12h13',
  } as const;

  /** Solid shapes drawn on top of the outline, for the few icons that need one. */
  const FILLS: Partial<Record<keyof typeof PATHS, string>> = {
    moon: 'M18.5 14.5A7 7 0 0 1 9.5 5.5a7 7 0 1 0 9 9z',
    auto: 'M12 4.5a7.5 7.5 0 0 1 0 15z',
  };

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  let { name, size = 18 }: { name: IconName; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.75"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <path d={PATHS[name]} />
  {#if FILLS[name]}<path d={FILLS[name]} fill="currentColor" stroke="none" />{/if}
</svg>
