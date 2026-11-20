import { describe, expect, it } from 'vitest'
import { isSafeHttpUrl, parseInline, parseMarkdown } from './markdown'

describe('markdown parser', () => {
  it('validates safe http/https URLs and rejects unsafe schemes and credentials', () => {
    expect(isSafeHttpUrl('https://github.com/Reiux/Lumen')).toBe(true)
    expect(isSafeHttpUrl('http://example.com/test')).toBe(true)
    expect(isSafeHttpUrl('javascript:alert(1)')).toBe(false)
    expect(isSafeHttpUrl('data:text/html,<script>alert(1)</script>')).toBe(false)
    expect(isSafeHttpUrl('file:///etc/passwd')).toBe(false)
    expect(isSafeHttpUrl('https://user:pass@github.com/Reiux/Lumen')).toBe(false)
    expect(isSafeHttpUrl('https://user@github.com/Reiux/Lumen')).toBe(false)
    expect(isSafeHttpUrl('https://:pass@github.com/Reiux/Lumen')).toBe(false)
  })

  it('parses headings, horizontal rules, paragraphs, and lists from GitHub release notes', () => {
    const md = [
      '# Lumen 2.1.1',
      '',
      'After shipping `2.1.0`, we fixed **edge cases** and *weird* bugs.',
      '',
      '---',
      '',
      '### Bug Fixes',
      '- Date patterns like `IMG_YYYYMMDD` and `PXL_` stay literal.',
      '- See [Release Notes](https://github.com/Reiux/Lumen/releases) for details.',
      '- [x] Checked task',
      '- [ ] Pending task',
    ].join('\n')

    const blocks = parseMarkdown(md)
    expect(blocks).toHaveLength(5)
    expect(blocks[0]).toEqual({
      type: 'heading',
      level: 1,
      children: [{ type: 'text', text: 'Lumen 2.1.1' }],
    })
    expect(blocks[1].type).toBe('paragraph')
    expect(blocks[2]).toEqual({ type: 'hr' })
    expect(blocks[3]).toEqual({
      type: 'heading',
      level: 3,
      children: [{ type: 'text', text: 'Bug Fixes' }],
    })
    expect(blocks[4].type).toBe('list')
    if (blocks[4].type === 'list') {
      expect(blocks[4].ordered).toBe(false)
      expect(blocks[4].items).toHaveLength(4)
      expect(blocks[4].items[0].children).toEqual([
        { type: 'text', text: 'Date patterns like ' },
        { type: 'code', text: 'IMG_YYYYMMDD' },
        { type: 'text', text: ' and ' },
        { type: 'code', text: 'PXL_' },
        { type: 'text', text: ' stay literal.' },
      ])
      expect(blocks[4].items[1].children).toEqual([
        { type: 'text', text: 'See ' },
        {
          type: 'link',
          href: 'https://github.com/Reiux/Lumen/releases',
          children: [{ type: 'text', text: 'Release Notes' }],
        },
        { type: 'text', text: ' for details.' },
      ])
      expect(blocks[4].items[2].checked).toBe(true)
      expect(blocks[4].items[3].checked).toBe(false)
    }
  })

  it('does not treat unsafe links or raw HTML tags as executable markup', () => {
    const inline = parseInline('Click [bad](javascript:alert(1)) or <script>alert(1)</script>')
    expect(inline).toEqual([
      { type: 'text', text: 'Click [bad](javascript:alert(1)) or <script>alert(1)</script>' },
    ])
  })

  it('preserves snake_case identifiers without italicizing underscores', () => {
    const inline = parseInline('Fixed lumen_tauri_build and _italic_ text')
    expect(inline).toEqual([
      { type: 'text', text: 'Fixed lumen_tauri_build and ' },
      { type: 'em', children: [{ type: 'text', text: 'italic' }] },
      { type: 'text', text: ' text' },
    ])
  })

  it('parses fenced code blocks, blockquotes, and ordered lists', () => {
    const md = [
      '> Important security note',
      '',
      '```bash',
      'lumen-cli --recursive=false',
      '```',
      '',
      '1. First step',
      '2. Second step',
    ].join('\n')

    const blocks = parseMarkdown(md)
    expect(blocks).toHaveLength(3)
    expect(blocks[0]).toEqual({
      type: 'blockquote',
      children: [{ type: 'text', text: 'Important security note' }],
    })
    expect(blocks[1]).toEqual({
      type: 'code',
      lang: 'bash',
      code: 'lumen-cli --recursive=false',
    })
    expect(blocks[2]).toEqual({
      type: 'list',
      ordered: true,
      items: [
        { checked: undefined, children: [{ type: 'text', text: 'First step' }] },
        { checked: undefined, children: [{ type: 'text', text: 'Second step' }] },
      ],
    })
  })

  it('prevents nested links and bare URLs inside link labels', () => {
    const nestedMd = parseInline('[outer [inner](https://inner.example)](https://outer.example)')
    expect(nestedMd).toEqual([
      {
        type: 'link',
        href: 'https://outer.example',
        children: [{ type: 'text', text: 'outer [inner](https://inner.example)' }],
      },
    ])

    const bareInsideLabel = parseInline('[See https://inner.example](https://outer.example)')
    expect(bareInsideLabel).toEqual([
      {
        type: 'link',
        href: 'https://outer.example',
        children: [{ type: 'text', text: 'See https://inner.example' }],
      },
    ])
  })

  it('preserves trailing # attached to words in headings while stripping spaced closing hashes', () => {
    const blocks = parseMarkdown('## Added C#\n\n## Section Title ##')
    expect(blocks).toEqual([
      {
        type: 'heading',
        level: 2,
        children: [{ type: 'text', text: 'Added C#' }],
      },
      {
        type: 'heading',
        level: 2,
        children: [{ type: 'text', text: 'Section Title' }],
      },
    ])
  })

  it('supports bold italic (***...***), escaped \\* inside emphasis, and balanced parentheses in URLs', () => {
    expect(parseInline('This is ***bold italic*** text')).toEqual([
      { type: 'text', text: 'This is ' },
      {
        type: 'strong',
        children: [{ type: 'em', children: [{ type: 'text', text: 'bold italic' }] }],
      },
      { type: 'text', text: ' text' },
    ])

    expect(parseInline('Note *2 \\* 3 = 6* here')).toEqual([
      { type: 'text', text: 'Note ' },
      { type: 'em', children: [{ type: 'text', text: '2 * 3 = 6' }] },
      { type: 'text', text: ' here' },
    ])

    expect(parseInline('See [Wikipedia](https://en.wikipedia.org/wiki/Foo_(bar)) now')).toEqual([
      { type: 'text', text: 'See ' },
      {
        type: 'link',
        href: 'https://en.wikipedia.org/wiki/Foo_(bar)',
        children: [{ type: 'text', text: 'Wikipedia' }],
      },
      { type: 'text', text: ' now' },
    ])

    expect(parseInline('Direct (https://en.wikipedia.org/wiki/Foo_(bar)).')).toEqual([
      { type: 'text', text: 'Direct (' },
      {
        type: 'link',
        href: 'https://en.wikipedia.org/wiki/Foo_(bar)',
        children: [{ type: 'text', text: 'https://en.wikipedia.org/wiki/Foo_(bar)' }],
      },
      { type: 'text', text: ').' },
    ])
  })
})
