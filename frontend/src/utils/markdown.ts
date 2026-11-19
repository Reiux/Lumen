export type MdInline =
  | { type: 'text'; text: string }
  | { type: 'code'; text: string }
  | { type: 'strong'; children: MdInline[] }
  | { type: 'em'; children: MdInline[] }
  | { type: 'del'; children: MdInline[] }
  | { type: 'link'; href: string; children: MdInline[] }

export interface MdListItem {
  checked?: boolean
  children: MdInline[]
}

export type MdBlock =
  | { type: 'heading'; level: 1 | 2 | 3 | 4; children: MdInline[] }
  | { type: 'hr' }
  | { type: 'code'; lang: string; code: string }
  | { type: 'blockquote'; children: MdInline[] }
  | { type: 'list'; ordered: boolean; items: MdListItem[] }
  | { type: 'paragraph'; children: MdInline[] }

/**
 * Only permit safe external web links (http/https). Rejects javascript:, data:, file:, etc.
 */
export function isSafeHttpUrl(raw: string): boolean {
  try {
    const parsed = new URL(raw.trim())
    return parsed.protocol === 'https:' || parsed.protocol === 'http:'
  } catch {
    return false
  }
}

/**
 * Parse inline GitHub-flavored Markdown tokens:
 * - Inline code: `code` (parsed first so underscores/asterisks inside code stay literal)
 * - Links: [label](https://...) and bare https://... URLs
 * - Bold: **text** or __text__
 * - Italic: *text* or _text_
 * - Strikethrough: ~~text~~
 */
export function parseInline(input: string, depth = 0): MdInline[] {
  if (!input) return []
  if (depth > 4) return [{ type: 'text', text: input }]

  const tokens: MdInline[] = []
  let i = 0
  let textBuf = ''

  const flushText = () => {
    if (textBuf) {
      tokens.push({ type: 'text', text: textBuf })
      textBuf = ''
    }
  }

  while (i < input.length) {
    // Escaped markdown punctuation (\*, \`, \[, etc.)
    if (input[i] === '\\' && i + 1 < input.length && /[\\`*_{}[\]()#+\-.!~]/.test(input[i + 1])) {
      textBuf += input[i + 1]
      i += 2
      continue
    }

    // Inline code: `...`
    if (input[i] === '`') {
      const end = input.indexOf('`', i + 1)
      if (end !== -1) {
        flushText()
        tokens.push({ type: 'code', text: input.slice(i + 1, end) })
        i = end + 1
        continue
      }
    }

    // Markdown link: [label](url)
    if (input[i] === '[') {
      const closeBracket = input.indexOf(']', i + 1)
      if (closeBracket !== -1 && input[closeBracket + 1] === '(') {
        const closeParen = input.indexOf(')', closeBracket + 2)
        if (closeParen !== -1) {
          const label = input.slice(i + 1, closeBracket)
          const href = input.slice(closeBracket + 2, closeParen).trim()
          if (isSafeHttpUrl(href)) {
            flushText()
            tokens.push({
              type: 'link',
              href,
              children: parseInline(label, depth + 1),
            })
            i = closeParen + 1
            continue
          }
        }
      }
    }

    // Bare https:// or http:// URL
    if ((input.startsWith('https://', i) || input.startsWith('http://', i)) && (i === 0 || /[\s(]/.test(input[i - 1]))) {
      const rest = input.slice(i)
      const match = rest.match(/^https?:\/\/[^\s<>)\]]+/)
      if (match) {
        let url = match[0]
        // Trim trailing sentence punctuation
        while (/[.,;:!?]$/.test(url)) {
          url = url.slice(0, -1)
        }
        if (isSafeHttpUrl(url)) {
          flushText()
          tokens.push({
            type: 'link',
            href: url,
            children: [{ type: 'text', text: url }],
          })
          i += url.length
          continue
        }
      }
    }

    // Strong: **text** or __text__
    if (input.startsWith('**', i) || (input.startsWith('__', i) && (i === 0 || !/\w/.test(input[i - 1])))) {
      const delim = input.slice(i, i + 2)
      const end = input.indexOf(delim, i + 2)
      if (end !== -1 && end > i + 2) {
        flushText()
        tokens.push({
          type: 'strong',
          children: parseInline(input.slice(i + 2, end), depth + 1),
        })
        i = end + 2
        continue
      }
    }

    // Strikethrough: ~~text~~
    if (input.startsWith('~~', i)) {
      const end = input.indexOf('~~', i + 2)
      if (end !== -1 && end > i + 2) {
        flushText()
        tokens.push({
          type: 'del',
          children: parseInline(input.slice(i + 2, end), depth + 1),
        })
        i = end + 2
        continue
      }
    }

    // Emphasis: *text* or _text_ (avoid splitting snake_case identifiers)
    if (
      (input[i] === '*' && input[i + 1] !== ' ' && input[i + 1] !== '*') ||
      (input[i] === '_' && input[i + 1] !== ' ' && input[i + 1] !== '_' && (i === 0 || !/\w/.test(input[i - 1])))
    ) {
      const delim = input[i]
      let end = input.indexOf(delim, i + 1)
      while (end !== -1 && delim === '_' && end + 1 < input.length && /\w/.test(input[end + 1])) {
        end = input.indexOf(delim, end + 1)
      }
      if (end !== -1 && end > i + 1 && input[end - 1] !== ' ') {
        flushText()
        tokens.push({
          type: 'em',
          children: parseInline(input.slice(i + 1, end), depth + 1),
        })
        i = end + 1
        continue
      }
    }

    textBuf += input[i]
    i++
  }

  flushText()
  return tokens
}

/**
 * Parse GitHub release-note Markdown into structured block AST nodes (zero v-html).
 */
export function parseMarkdown(source: string): MdBlock[] {
  if (!source || !source.trim()) return []

  const lines = source.replace(/\r\n?/g, '\n').split('\n')
  const blocks: MdBlock[] = []
  let idx = 0

  const isBlockBoundary = (line: string): boolean => {
    const trimmed = line.trim()
    return (
      trimmed === '' ||
      /^```/.test(trimmed) ||
      /^#{1,6}\s+/.test(trimmed) ||
      /^(-{3,}|\*{3,}|_{3,})$/.test(trimmed) ||
      /^>\s?/.test(trimmed) ||
      /^\s*[-*+]\s+/.test(line) ||
      /^\s*\d+\.\s+/.test(line)
    )
  }

  while (idx < lines.length) {
    const rawLine = lines[idx]
    const trimmed = rawLine.trim()

    if (!trimmed) {
      idx++
      continue
    }

    // Fenced code block: ```lang
    if (trimmed.startsWith('```')) {
      const lang = trimmed.slice(3).trim()
      const codeLines: string[] = []
      idx++
      while (idx < lines.length && !lines[idx].trim().startsWith('```')) {
        codeLines.push(lines[idx])
        idx++
      }
      if (idx < lines.length) idx++ // consume closing fence
      blocks.push({ type: 'code', lang, code: codeLines.join('\n') })
      continue
    }

    // Horizontal rule: ---, ***, ___
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
      blocks.push({ type: 'hr' })
      idx++
      continue
    }

    // Headings: # .. ######
    const headingMatch = trimmed.match(/^(#{1,6})\s+(.+?)\s*#*$/)
    if (headingMatch) {
      const rawLevel = headingMatch[1].length
      const level = (rawLevel <= 4 ? rawLevel : 4) as 1 | 2 | 3 | 4
      blocks.push({
        type: 'heading',
        level,
        children: parseInline(headingMatch[2]),
      })
      idx++
      continue
    }

    // Blockquote: > ...
    if (/^>\s?/.test(trimmed)) {
      const quoteLines: string[] = []
      while (idx < lines.length && /^>\s?/.test(lines[idx].trim())) {
        quoteLines.push(lines[idx].trim().replace(/^>\s?/, ''))
        idx++
      }
      blocks.push({
        type: 'blockquote',
        children: parseInline(quoteLines.join(' ')),
      })
      continue
    }

    // Unordered list: - item, * item, + item (plus GFM task checkboxes)
    if (/^\s*[-*+]\s+/.test(rawLine)) {
      const items: MdListItem[] = []
      while (idx < lines.length && /^\s*[-*+]\s+/.test(lines[idx])) {
        let itemText = lines[idx].replace(/^\s*[-*+]\s+/, '').trim()
        let checked: boolean | undefined
        const taskMatch = itemText.match(/^\[([ xX])\]\s+(.*)$/)
        if (taskMatch) {
          checked = taskMatch[1].toLowerCase() === 'x'
          itemText = taskMatch[2]
        }
        idx++
        // Consume wrapped continuation lines
        while (idx < lines.length && lines[idx].trim() !== '' && !isBlockBoundary(lines[idx])) {
          itemText += ' ' + lines[idx].trim()
          idx++
        }
        items.push({ checked, children: parseInline(itemText) })
      }
      blocks.push({ type: 'list', ordered: false, items })
      continue
    }

    // Ordered list: 1. item
    if (/^\s*\d+\.\s+/.test(rawLine)) {
      const items: MdListItem[] = []
      while (idx < lines.length && /^\s*\d+\.\s+/.test(lines[idx])) {
        let itemText = lines[idx].replace(/^\s*\d+\.\s+/, '').trim()
        idx++
        while (idx < lines.length && lines[idx].trim() !== '' && !isBlockBoundary(lines[idx])) {
          itemText += ' ' + lines[idx].trim()
          idx++
        }
        items.push({ children: parseInline(itemText) })
      }
      blocks.push({ type: 'list', ordered: true, items })
      continue
    }

    // Paragraph: collect contiguous non-boundary lines
    const paraLines: string[] = [trimmed]
    idx++
    while (idx < lines.length && !isBlockBoundary(lines[idx])) {
      paraLines.push(lines[idx].trim())
      idx++
    }
    blocks.push({
      type: 'paragraph',
      children: parseInline(paraLines.join(' ')),
    })
  }

  return blocks
}
