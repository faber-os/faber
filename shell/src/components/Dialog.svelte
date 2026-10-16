<script lang="ts" module>
  let count = 0;
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';

  /**
   * A sheet over one window (or the desktop): it blocks only what it covers, like a macOS sheet,
   * so a question in Notes never freezes the rest of the shell. Escape cancels.
   */
  let {
    title,
    oncancel,
    children,
    actions,
    tone = 'normal',
    wide = false,
  }: {
    title: string;
    oncancel: () => void;
    children?: Snippet;
    actions: Snippet;
    tone?: 'normal' | 'danger';
    wide?: boolean;
  } = $props();

  const id = `dialog-${++count}`;
  let box: HTMLDivElement | undefined = $state();
  const before = typeof document === 'undefined' ? null : document.activeElement;

  $effect(() => {
    if (!box) return;
    const first =
      box.querySelector<HTMLElement>('[data-autofocus]') ??
      box.querySelector<HTMLElement>('input, textarea, select') ??
      box.querySelector<HTMLElement>('.actions button:last-child');
    (first ?? box).focus();
    return () => {
      if (before instanceof HTMLElement && before.isConnected)
        before.focus({ preventScroll: true });
    };
  });

  function focusables(): HTMLElement[] {
    return [
      ...(box?.querySelectorAll<HTMLElement>(
        'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [href], [tabindex="0"]',
      ) ?? []),
    ];
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      oncancel();
      return;
    }
    if (e.key !== 'Tab') return;
    const all = focusables();
    if (all.length === 0) return;
    const i = all.indexOf(document.activeElement as HTMLElement);
    if (e.shiftKey && i <= 0) {
      e.preventDefault();
      all.at(-1)!.focus();
    } else if (!e.shiftKey && i === all.length - 1) {
      e.preventDefault();
      all[0]!.focus();
    }
  }
</script>

<div class="scrim">
  <div
    bind:this={box}
    class="box {tone}"
    class:wide
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="{id}-title"
    tabindex="-1"
    {onkeydown}
  >
    <h3 id="{id}-title" class="title">{title}</h3>
    {#if children}<div class="body">{@render children()}</div>{/if}
    <div class="actions">{@render actions()}</div>
  </div>
</div>

<style>
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: start center;
    padding: var(--space-6) var(--space-3) var(--space-3);
    overflow-y: auto;
    background: var(--scrim);
    animation: fade var(--dur-fast) var(--ease-out);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .box {
    display: grid;
    gap: var(--space-3);
    width: min(420px, 100%);
    padding: var(--space-5);
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
    outline: none;
    user-select: text;
  }
  .box.wide {
    width: min(540px, 100%);
  }
  .title {
    font-size: var(--text-lg);
    font-weight: 700;
    letter-spacing: -0.005em;
  }
  .danger .title {
    color: var(--danger-text);
  }
  .body {
    display: grid;
    gap: var(--space-3);
    font-size: var(--text-md);
    color: var(--text);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
</style>
