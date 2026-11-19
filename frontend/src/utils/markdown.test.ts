import { describe, expect, it } from 'vitest'
import { isSafeHttpUrl, parseInline, parseMarkdown } from './markdown'

describe('markdown parser', () => {
  it('validates safe http/https URLs and rejects unsafe schemes', () => {
    expect(isSafeHttpUrl('https://github.com/Reiux/Lumen')).toBe(true)
    expect(isSafeHttpUrl('http://example.com/test')).toBe(true)
    expect(isSafeHttpUrl('javascript:alert(1)')).toBe(false)
    expect(isSafeHttpUrl('data:text/html,<script>alert(1)</script>')).toBe(false)
    expect(isSafeHttpUrl('file:///etc/passwd')).toBe(false)
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
})
