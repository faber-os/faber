import { describe, expect, it } from 'vitest';
import { parseInline, parseMarkdown, safeLink } from './markdown';

describe('parseMarkdown', () => {
  it('reads headings, paragraphs, lists, code and tables', () => {
    const text = [
      '# My sensor',
      '',
      'Reads `D:123` lines',
      'over **serial**.',
      '',
      '## Steps',
      '1. Install it.',
      '2. Add a part,',
      '   on its port.',
      '- one',
      '* two',
      '',
      '```cpp',
      'Serial.begin(9600);',
      '```',
      '| Sensor | Arduino |',
      '|---|---|',
      '| TX | RX |',
      '> Careful.',
    ].join('\n');
    expect(parseMarkdown(text)).toEqual([
      { kind: 'heading', level: 1, inline: [{ kind: 'text', text: 'My sensor' }] },
      {
        kind: 'paragraph',
        inline: [
          { kind: 'text', text: 'Reads ' },
          { kind: 'code', text: 'D:123' },
          { kind: 'text', text: ' lines over ' },
          { kind: 'strong', text: 'serial' },
          { kind: 'text', text: '.' },
        ],
      },
      { kind: 'heading', level: 2, inline: [{ kind: 'text', text: 'Steps' }] },
      {
        kind: 'list',
        ordered: true,
        items: [
          [{ kind: 'text', text: 'Install it.' }],
          [{ kind: 'text', text: 'Add a part, on its port.' }],
        ],
      },
      {
        kind: 'list',
        ordered: false,
        items: [[{ kind: 'text', text: 'one' }], [{ kind: 'text', text: 'two' }]],
      },
      { kind: 'code', text: 'Serial.begin(9600);' },
      {
        kind: 'table',
        rows: [
          [[{ kind: 'text', text: 'Sensor' }], [{ kind: 'text', text: 'Arduino' }]],
          [[{ kind: 'text', text: 'TX' }], [{ kind: 'text', text: 'RX' }]],
        ],
      },
      { kind: 'quote', inline: [{ kind: 'text', text: 'Careful.' }] },
    ]);
  });

  it('keeps raw HTML as text, never as markup', () => {
    const blocks = parseMarkdown('<script>alert(1)</script>\n<img src=x onerror=alert(1)>');
    expect(blocks).toEqual([
      {
        kind: 'paragraph',
        inline: [{ kind: 'text', text: '<script>alert(1)</script> <img src=x onerror=alert(1)>' }],
      },
    ]);
  });

  it('keeps an unclosed code block to the end', () => {
    expect(parseMarkdown('```\na\nb')).toEqual([{ kind: 'code', text: 'a\nb' }]);
  });
});

describe('parseInline', () => {
  it('reads emphasis, code and links', () => {
    expect(parseInline('a *b* _c_ [docs](https://faber.dev/x) `**not bold**`')).toEqual([
      { kind: 'text', text: 'a ' },
      { kind: 'em', text: 'b' },
      { kind: 'text', text: ' ' },
      { kind: 'em', text: 'c' },
      { kind: 'text', text: ' ' },
      { kind: 'link', text: 'docs', href: 'https://faber.dev/x' },
      { kind: 'text', text: ' ' },
      { kind: 'code', text: '**not bold**' },
    ]);
  });

  it('turns links that are not http(s) into plain text', () => {
    expect(parseInline('[click](javascript:alert(1))')).toEqual([{ kind: 'text', text: 'click' }]);
    expect(parseInline('[a](data:text/html,x)')).toEqual([{ kind: 'text', text: 'a' }]);
  });

  it('leaves a lone marker as it is', () => {
    expect(parseInline('5 * 3 and a_b')).toEqual([{ kind: 'text', text: '5 * 3 and a_b' }]);
  });
});

describe('safeLink', () => {
  it('allows only http and https', () => {
    expect(safeLink('https://example.com')).toBe('https://example.com');
    expect(safeLink('HTTP://example.com')).toBe('HTTP://example.com');
    expect(safeLink(' javascript:alert(1)')).toBeNull();
    expect(safeLink('./README.md')).toBeNull();
  });
});
