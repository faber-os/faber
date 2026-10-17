<script lang="ts">
  /** A right-click menu with submenus: a stack of levels, each opened from an item of the last. */
  import MenuLevel from './MenuLevel.svelte';
  import type { MenuItem } from './menuItem';

  let { items, x, y, onclose }: { items: MenuItem[]; x: number; y: number; onclose: () => void } =
    $props();

  interface Level {
    items: MenuItem[];
    x: number;
    y: number;
    /** The item of the level before that opened this one. */
    from: number;
  }
  // svelte-ignore state_referenced_locally (a menu is made anew for every opening: CircuitEditor keys it)
  let levels = $state<Level[]>([{ items, x, y, from: -1 }]);
  const refs: (MenuLevel | undefined)[] = $state([]);
  let widths: number[] = $state([]);

  function sub(depth: number, index: number | null, sx: number, sy: number) {
    const keep = levels.slice(0, depth + 1);
    const children = index === null ? undefined : levels[depth]!.items[index]?.children;
    levels = children ? [...keep, { items: children, x: sx, y: sy, from: index! }] : keep;
  }

  function back(depth: number) {
    const from = levels[depth]!.from;
    levels = levels.slice(0, depth);
    refs[depth - 1]?.focusAt(from);
  }
</script>

{#each levels as lv, d (`${d}:${lv.from}`)}
  <div class="level" bind:clientWidth={widths[d]}>
    <MenuLevel
      bind:this={refs[d]}
      items={lv.items}
      x={lv.x}
      y={lv.y}
      depth={d}
      parentWidth={d ? (widths[d - 1] ?? 0) : 0}
      openIndex={levels[d + 1]?.from ?? null}
      {onclose}
      onback={() => back(d)}
      onsub={(i, sx, sy) => sub(d, i, sx, sy)}
    />
  </div>
{/each}

<style>
  .level {
    display: contents;
  }
</style>
