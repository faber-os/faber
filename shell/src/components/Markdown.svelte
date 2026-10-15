<script lang="ts">
  import { parseMarkdown, type Inline } from '../lib/desktop/markdown';

  /**
   * Safe minimal Markdown (lib/desktop/markdown.ts), e.g. a package's README. Every piece is
   * drawn as an element with text in it: nothing is ever read as HTML, so it can't run script or
   * load anything. Links open in a new tab, and only http(s) links are links at all.
   */
  let {
    text,
    headingLevel = 4,
  }: {
    text: string;
    /** The level a `#` heading gets in the page's outline, below the heading it sits under. */
    headingLevel?: number;
  } = $props();

  const blocks = $derived(parseMarkdown(text));
</script>

{#snippet spans(inline: Inline[])}
  {#each inline as s, i (i)}
    {#if s.kind === 'code'}<code>{s.text}</code>
    {:else if s.kind === 'strong'}<strong>{s.text}</strong>
    {:else if s.kind === 'em'}<em>{s.text}</em>
    {:else if s.kind === 'link'}<a href={s.href} target="_blank" rel="noopener noreferrer nofollow"
        >{s.text}</a
      >
    {:else}{s.text}{/if}
  {/each}
{/snippet}

<div class="markdown">
  {#each blocks as b, i (i)}
    {#if b.kind === 'heading'}
      <p
        class="heading h{Math.min(b.level, 3)}"
        role="heading"
        aria-level={headingLevel + b.level - 1}
      >
        {@render spans(b.inline)}
      </p>
    {:else if b.kind === 'paragraph'}
      <p>{@render spans(b.inline)}</p>
    {:else if b.kind === 'quote'}
      <blockquote>{@render spans(b.inline)}</blockquote>
    {:else if b.kind === 'list'}
      {#if b.ordered}
        <ol>
          {#each b.items as item, j (j)}<li>{@render spans(item)}</li>{/each}
        </ol>
      {:else}
        <ul>
          {#each b.items as item, j (j)}<li>{@render spans(item)}</li>{/each}
        </ul>
      {/if}
    {:else if b.kind === 'code'}
      <pre><code>{b.text}</code></pre>
    {:else if b.kind === 'table'}
      <div class="table-wrap">
        <table>
          <tbody>
            {#each b.rows as row, r (r)}
              <tr>
                {#each row as cell, c (c)}
                  {#if r === 0}<th>{@render spans(cell)}</th>{:else}<td>{@render spans(cell)}</td
                    >{/if}
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/each}
</div>

<style>
  .markdown {
    display: grid;
    gap: var(--space-2);
    font-size: var(--text-sm);
    line-height: 1.5;
    min-width: 0;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .heading {
    font-weight: 700;
    margin-top: var(--space-2);
  }
  .h1 {
    font-size: var(--text-lg);
  }
  .h2 {
    font-size: var(--text-md);
  }
  ul,
  ol {
    margin: 0;
    padding-left: 1.4em;
  }
  blockquote {
    margin: 0;
    padding-left: var(--space-3);
    border-left: 3px solid var(--line);
    color: var(--text-muted);
  }
  code {
    font-family: var(--font-data);
    font-size: 0.95em;
  }
  pre {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--surface-sunken, var(--hover));
    overflow-x: auto;
    white-space: pre;
  }
  .table-wrap {
    overflow-x: auto;
  }
  table {
    border-collapse: collapse;
  }
  th,
  td {
    padding: 2px var(--space-2);
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: top;
  }
</style>
