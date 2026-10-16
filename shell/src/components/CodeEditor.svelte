<script lang="ts">
  import { onMount } from 'svelte';
  import type { CompilerMessage, Editor, SketchFile } from './codemirror';

  const NONE: readonly CompilerMessage[] = [];

  /**
   * A code editor for a sketch's tab (CodeMirror 6, set up in codemirror.ts and loaded the first
   * time an editor opens). The parent owns the text: `value` is read once, every change goes to
   * `onchange`, and `goto` moves the cursor (to an error's line). `diagnostics` are the
   * compiler's messages for this tab, underlined in the code; a new list replaces them.
   */
  let {
    value,
    label,
    file,
    others = () => [],
    diagnostics = NONE,
    visible = true,
    onchange,
    readOnly = false,
  }: {
    value: string;
    /** For screen readers: which file this is. */
    label: string;
    /** The tab's file name, e.g. `Rover.ino`. */
    file: string;
    /** The sketch's other tabs, for completion and hover to know their names. */
    others?: () => SketchFile[];
    diagnostics?: readonly CompilerMessage[];
    /** False while another tab is shown; the editor measures itself again when it is shown. */
    visible?: boolean;
    onchange: (text: string) => void;
    /** Read once: shown but not editable (a guided board's generated sketch). */
    readOnly?: boolean;
  } = $props();

  let host: HTMLDivElement;
  let editor = $state<Editor | undefined>();
  /** A jump asked for before the editor had loaded. */
  let pending: [number, number] | null = null;
  /** The list of messages the editor shows: only a new list replaces them, so the underlines
   *  an edit took away don't come back. */
  let shown: readonly CompilerMessage[] = NONE;

  /** Puts the cursor at a line (and column), scrolled into view, and focuses the editor. */
  export function goto(line: number, column = 1): void {
    if (editor) editor.goto(line, column);
    else pending = [line, column];
  }

  onMount(() => {
    let gone = false;
    void import('./codemirror').then(({ createEditor }) => {
      if (gone) return;
      // svelte-ignore state_referenced_locally (read once: the parent owns the text from here)
      editor = createEditor(host, value, {
        label,
        file,
        others: () => others(),
        onchange: (text) => onchange(text),
        readOnly,
      });
      if (pending) editor.goto(...pending);
      pending = null;
    });
    return () => {
      gone = true;
      editor?.destroy();
    };
  });

  $effect(() => {
    const list = diagnostics;
    if (!editor || list === shown) return;
    shown = list;
    editor.setDiagnostics(list);
  });

  // A tab that was hidden has the wrong size cached: measure again once it shows.
  $effect(() => {
    if (visible) editor?.measure();
  });
</script>

<div class="code-editor" bind:this={host} hidden={!visible}></div>

<style>
  .code-editor {
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .code-editor[hidden] {
    display: none;
  }
  .code-editor :global(.cm-editor) {
    flex: 1;
    min-height: 0;
  }
  .code-editor :global(.cm-editor.cm-focused) {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
</style>
