<script setup lang="ts">
import { ref, computed } from 'vue'
import IconRenderer from './IconRenderer.vue'

const props = withDefaults(
  defineProps<{
    content: string
    isDark?: boolean
    streaming?: boolean
  }>(),
  {
    content: '',
    isDark: false,
    streaming: false
  }
)

const emit = defineEmits<{
  (e: 'copy-code', payload: { code: string; lang: string }): void
  (e: 'send-to-plugin', payload: { code: string; target: 'json-studio' | 'diff-viewer' | 'calc-pad' }): void
}>()

const copiedBlockIdx = ref<number | null>(null)
const wrapBlockMap = ref<Record<number, boolean>>({})

interface MdNode {
  type:
    | 'heading'
    | 'paragraph'
    | 'code'
    | 'table'
    | 'blockquote'
    | 'ul'
    | 'ol'
    | 'hr'
  level?: number
  text?: string
  lang?: string
  code?: string
  headers?: string[]
  aligns?: ('left' | 'center' | 'right')[]
  rows?: string[][]
  items?: string[]
}

function escapeHtml(raw: string): string {
  return raw
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

/**
 * Render inline Markdown syntax safely into HTML:
 * - Inline code: `code`
 * - Bold + Italic: ***text***
 * - Bold: **text**
 * - Italic: *text*
 * - Strikethrough: ~~text~~
 * - Links: [label](https://...)
 */
function renderInlineMarkdown(raw: string, isDark: boolean): string {
  if (!raw) return ''

  const savedTokens: string[] = []
  const saveInlineToken = (html: string) => {
    const idx = savedTokens.length
    savedTokens.push(html)
    return `\u0000INL_${idx}\u0000`
  }

  // 1. Extract inline code spans (`code`) first so inner symbols (*, _, ~, etc.) are never mutated
  let text = raw.replace(/`([^`\n]+)`/g, (_, codeContent: string) => {
    const cls = isDark
      ? 'px-1.5 py-0.5 mx-0.5 rounded-md font-mono text-[11.5px] bg-indigo-500/15 text-indigo-300 border border-indigo-500/25'
      : 'px-1.5 py-0.5 mx-0.5 rounded-md font-mono text-[11.5px] bg-indigo-50/90 text-indigo-700 border border-indigo-200/70'
    return saveInlineToken(`<code class="${cls}">${escapeHtml(codeContent)}</code>`)
  })

  // 2. Extract Markdown Links (`[text](url)`) before escaping/formatting so URLs with `~` or `*` are safe
  text = text.replace(
    /\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,
    (_, label: string, url: string) => {
      return saveInlineToken(
        `<a href="${escapeHtml(url)}" target="_blank" rel="noopener noreferrer" class="text-indigo-500 hover:underline font-medium">${escapeHtml(label)}</a>`
      )
    }
  )

  // 3. Escape remaining HTML
  text = escapeHtml(text)

  // 4. Bold + Italic (`***...***`)
  text = text.replace(/\*\*\*([^*]+)\*\*\*/g, (_, inner: string) =>
    saveInlineToken(`<strong class="font-bold italic">${inner}</strong>`)
  )

  // 5. Bold (`**...**`)
  const boldCls = isDark ? 'font-semibold text-white' : 'font-semibold text-slate-900'
  text = text.replace(/\*\*([^*]+)\*\*/g, (_, inner: string) =>
    saveInlineToken(`<strong class="${boldCls}">${inner}</strong>`)
  )

  // 6. Italic (`*...*`)
  text = text.replace(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, (_, pre: string, inner: string) => {
    return `${pre}${saveInlineToken(`<em class="italic opacity-95">${inner}</em>`)}`
  })

  // 7. Strikethrough (`~~...~~`)
  text = text.replace(/~~([^~\n]+)~~/g, (_, inner: string) =>
    saveInlineToken(`<del class="line-through opacity-65">${inner}</del>`)
  )

  // 8. Restore all inline tokens (run twice in case of nested inline tokens inside bold/italic)
  for (let pass = 0; pass < 2; pass++) {
    text = text.replace(/\u0000INL_(\d+)\u0000/g, (_, idxStr: string) => {
      return savedTokens[Number(idxStr)] || ''
    })
  }

  return text
}

/**
 * Single-pass lexical syntax highlighter supporting Light & Dark themes.
 * Scans raw source code in one pass from left to right and escapes HTML per token,
 * guaranteeing that generated `<span class="...">` tags are NEVER re-matched or corrupted.
 */
function highlightCodeHtml(code: string, lang: string, isDark: boolean): string {
  if (!code) return ''
  const lowerLang = (lang || 'text').toLowerCase()

  // 1. Special single-pass treatment for ASCII architecture / tree diagrams (`├──`, `└──`, `│`, `───>`)
  if (
    lowerLang === 'text' ||
    lowerLang === 'txt' ||
    lowerLang === 'plaintext' ||
    lowerLang === 'ascii' ||
    /[├└│─┌┐┘┬┴┼]/.test(code)
  ) {
    const treeSymCls = isDark ? 'text-indigo-400 font-bold' : 'text-indigo-600 font-bold'
    const protoCls = isDark ? 'text-emerald-400 font-semibold' : 'text-emerald-700 font-semibold'
    const numCls = isDark ? 'text-amber-300' : 'text-amber-700 font-medium'

    const asciiRegex =
      /([├└│─┌┐┘┬┴┼]+|---+>|===+>|<---+)|(\b(?:HTTP\/[123](?:\.\d)?|QUIC|TCP|UDP|TLS(?:\s*1\.[23])?|IPv[46]|DNS|Stream(?:\s*\d+)?|HTML|CSS|JavaScript|JSON|SQL)\b)|(\b\d+(?:\.\d+)?(?:ms|s|MB|GB|KB|%)?\b)/g

    let out = ''
    let lastIdx = 0
    let m: RegExpExecArray | null
    while ((m = asciiRegex.exec(code)) !== null) {
      if (m.index > lastIdx) {
        out += escapeHtml(code.slice(lastIdx, m.index))
      }
      const full = escapeHtml(m[0])
      if (m[1]) {
        out += `<span class="${treeSymCls}">${full}</span>`
      } else if (m[2]) {
        out += `<span class="${protoCls}">${full}</span>`
      } else {
        out += `<span class="${numCls}">${full}</span>`
      }
      lastIdx = asciiRegex.lastIndex
    }
    if (lastIdx < code.length) {
      out += escapeHtml(code.slice(lastIdx))
    }
    return out
  }

  // 2. Single-pass lexical scanner for programming languages & shell scripts
  const commentCls = isDark ? 'text-slate-500 italic' : 'text-slate-400 italic'
  const stringCls = isDark ? 'text-emerald-300' : 'text-emerald-700'
  const keyCls = isDark ? 'text-sky-300 font-medium' : 'text-sky-700 font-medium'
  const keywordCls = isDark ? 'text-violet-400 font-semibold' : 'text-purple-700 font-semibold'
  const numCls = isDark ? 'text-amber-300' : 'text-amber-700'
  const fnCls = isDark ? 'text-indigo-300' : 'text-indigo-700'
  const flagCls = isDark ? 'text-cyan-300 font-medium' : 'text-teal-700 font-medium'

  const isHashCommentLang = [
    'bash',
    'sh',
    'zsh',
    'shell',
    'console',
    'python',
    'py',
    'yaml',
    'yml',
    'toml',
    'dockerfile',
    'conf',
    'ini'
  ].includes(lowerLang)
  const isSqlLang = lowerLang === 'sql'
  const isJsonLang = lowerLang === 'json'

  // Build comment pattern branch based on language
  const commentBranch = isJsonLang
    ? '(?!x)x'
    : isHashCommentLang
      ? '(#[^\\n]*)'
      : isSqlLang
        ? '(--[^\\n]*|\\/\\*[\\s\\S]*?\\*\\/)'
        : '(\\/\\/[^\\n]*|\\/\\*[\\s\\S]*?\\*\\/)'

  // Group 1: Comment
  // Group 2: JSON / Object Key (`"key"` before `:`)
  // Group 3: String literal (`"..."`, `'...'`, `` `...` ``)
  // Group 4: Leading whitespace before CLI flag (so `-nP` in `lsof -nP` is matched cleanly, while `a-b` is not)
  // Group 5: CLI flag (`-nP`, `-iTCP`, `-sTCP:LISTEN`, `--max-time`)
  // Group 6: Language Keyword / Shell Command
  // Group 7: Function call identifier (`foo` before `(`)
  // Group 8: Number literal (`123`, `0x1f`, `3.14`)
  const tokenRegex = new RegExp(
    [
      commentBranch,
      '("(?:\\\\.|[^"\\\\\\n])*"(?=\\s*:))',
      '("(?:\\\\.|[^"\\\\\\n])*"|\'(?:\\\\.|[^\'\\\\\\n])*\'|`(?:\\\\.|[^`\\\\\\n])*`)',
      '((?:^|[ \\t])+)(--?[a-zA-Z0-9_.:=-]+)',
      '\\b(const|let|var|function|return|async|await|if|else|for|while|switch|case|break|continue|import|from|export|default|interface|type|class|extends|implements|new|try|catch|finally|throw|true|false|null|undefined|def|elif|except|lambda|yield|with|as|pass|None|True|False|fn|pub|mut|impl|struct|enum|match|use|mod|where|SELECT|FROM|WHERE|INSERT|INTO|UPDATE|SET|DELETE|JOIN|LEFT|INNER|ON|GROUP|BY|ORDER|LIMIT|AS|AND|OR|NOT|NULL|CREATE|TABLE|DROP|ALTER|INDEX|sudo|lsof|netstat|ss|ps|top|htop|kill|killall|pkill|curl|wget|grep|egrep|awk|sed|cat|head|tail|less|echo|printf|cd|ls|pwd|cp|mv|rm|mkdir|chmod|chown|find|xargs|sort|uniq|wc|tar|zip|unzip|ssh|scp|rsync|git|docker|kubectl|npm|pnpm|yarn|cargo|brew|systemctl|journalctl|ifconfig|ip|ping|dig|nslookup|nc|telnet|openssl|python3?|node|go)\\b',
      '\\b([a-zA-Z_][a-zA-Z0-9_]*)(?=\\s*\\()',
      '\\b(0x[0-9a-fA-F]+|\\d+(?:\\.\\d+)?)\\b'
    ].join('|'),
    'gm'
  )

  let out = ''
  let lastIdx = 0
  let match: RegExpExecArray | null

  while ((match = tokenRegex.exec(code)) !== null) {
    if (match.index > lastIdx) {
      out += escapeHtml(code.slice(lastIdx, match.index))
    }

    if (match[1] !== undefined) {
      // Comment
      out += `<span class="${commentCls}">${escapeHtml(match[1])}</span>`
    } else if (match[2] !== undefined) {
      // JSON / Object key
      out += `<span class="${keyCls}">${escapeHtml(match[2])}</span>`
    } else if (match[3] !== undefined) {
      // String literal
      out += `<span class="${stringCls}">${escapeHtml(match[3])}</span>`
    } else if (match[5] !== undefined) {
      // CLI flag with leading whitespace
      const lead = escapeHtml(match[4] || '')
      const flag = escapeHtml(match[5])
      out += `${lead}<span class="${flagCls}">${flag}</span>`
    } else if (match[6] !== undefined) {
      // Keyword or Shell command
      out += `<span class="${keywordCls}">${escapeHtml(match[6])}</span>`
    } else if (match[7] !== undefined) {
      // Function call
      out += `<span class="${fnCls}">${escapeHtml(match[7])}</span>`
    } else if (match[8] !== undefined) {
      // Number literal
      out += `<span class="${numCls}">${escapeHtml(match[8])}</span>`
    } else {
      out += escapeHtml(match[0])
    }

    lastIdx = tokenRegex.lastIndex
  }

  if (lastIdx < code.length) {
    out += escapeHtml(code.slice(lastIdx))
  }

  return out
}

function parseTableRow(line: string): string[] {
  const trimmed = line.trim().replace(/^\|/, '').replace(/\|$/, '')
  return trimmed.split('|').map((c) => c.trim())
}

function isTableSeparatorLine(line: string): boolean {
  const t = line.trim()
  if (!t.includes('|') || !t.includes('-')) return false
  const cells = parseTableRow(t)
  return cells.length > 0 && cells.every((c) => /^:?-{2,}:?$/.test(c))
}

/**
 * Parse full Markdown string into block AST nodes (Headings, Tables, Code Blocks, Blockquotes, Lists, HR, Paragraphs)
 */
function parseMarkdownAst(raw: string): MdNode[] {
  if (!raw) return []
  const nodes: MdNode[] = []
  const lines = raw.replace(/\r\n/g, '\n').split('\n')
  let i = 0

  while (i < lines.length) {
    const line = lines[i]
    const trimmed = line.trim()

    // 1. Blank line
    if (!trimmed) {
      i++
      continue
    }

    // 2. Fenced Code Block (` ```lang `)
    const fenceMatch = trimmed.match(/^```([a-zA-Z0-9_+-]*)\s*$/)
    if (fenceMatch) {
      const lang = (fenceMatch[1] || 'text').toLowerCase()
      const codeLines: string[] = []
      i++
      while (i < lines.length && !lines[i].trim().startsWith('```')) {
        codeLines.push(lines[i])
        i++
      }
      if (i < lines.length) i++ // skip closing ```
      nodes.push({
        type: 'code',
        lang,
        code: codeLines.join('\n')
      })
      continue
    }

    // 3. Horizontal Rule (`---`, `***`, `___`)
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
      nodes.push({ type: 'hr' })
      i++
      continue
    }

    // 4. Heading (`#` ~ `######`)
    const headingMatch = trimmed.match(/^(#{1,6})\s+(.+)$/)
    if (headingMatch) {
      nodes.push({
        type: 'heading',
        level: headingMatch[1].length,
        text: headingMatch[2].trim()
      })
      i++
      continue
    }

    // 5. GFM Table (`| a | b |` followed by `|---|---|`)
    if (
      trimmed.includes('|') &&
      i + 1 < lines.length &&
      isTableSeparatorLine(lines[i + 1])
    ) {
      const headers = parseTableRow(trimmed)
      const sepCells = parseTableRow(lines[i + 1])
      const aligns: ('left' | 'center' | 'right')[] = sepCells.map((c) => {
        if (c.startsWith(':') && c.endsWith(':')) return 'center'
        if (c.endsWith(':')) return 'right'
        return 'left'
      })
      i += 2
      const rows: string[][] = []
      while (i < lines.length && lines[i].trim().includes('|') && lines[i].trim() !== '') {
        rows.push(parseTableRow(lines[i]))
        i++
      }
      nodes.push({
        type: 'table',
        headers,
        aligns,
        rows
      })
      continue
    }

    // 6. Blockquote (`> ...`)
    if (trimmed.startsWith('>')) {
      const quoteLines: string[] = []
      while (i < lines.length && lines[i].trim().startsWith('>')) {
        quoteLines.push(lines[i].trim().replace(/^>\s?/, ''))
        i++
      }
      nodes.push({
        type: 'blockquote',
        text: quoteLines.join('\n')
      })
      continue
    }

    // 7. Unordered List (`- ...`, `* ...`, `• ...`)
    if (/^([-*•])\s+/.test(trimmed)) {
      const items: string[] = []
      while (i < lines.length && /^([-*•])\s+/.test(lines[i].trim())) {
        items.push(lines[i].trim().replace(/^([-*•])\s+/, ''))
        i++
      }
      nodes.push({
        type: 'ul',
        items
      })
      continue
    }

    // 8. Ordered List (`1. ...`)
    if (/^\d+\.\s+/.test(trimmed)) {
      const items: string[] = []
      while (i < lines.length && /^\d+\.\s+/.test(lines[i].trim())) {
        items.push(lines[i].trim().replace(/^\d+\.\s+/, ''))
        i++
      }
      nodes.push({
        type: 'ol',
        items
      })
      continue
    }

    // 9. Regular Paragraph (merge consecutive plain lines until blank or block element)
    const paraLines: string[] = [line]
    i++
    while (i < lines.length) {
      const nextTrim = lines[i].trim()
      if (
        !nextTrim ||
        nextTrim.startsWith('```') ||
        /^#{1,6}\s+/.test(nextTrim) ||
        /^(-{3,}|\*{3,}|_{3,})$/.test(nextTrim) ||
        nextTrim.startsWith('>') ||
        /^([-*•]|\d+\.)\s+/.test(nextTrim) ||
        (nextTrim.includes('|') && i + 1 < lines.length && isTableSeparatorLine(lines[i + 1]))
      ) {
        break
      }
      paraLines.push(lines[i])
      i++
    }
    nodes.push({
      type: 'paragraph',
      text: paraLines.join('\n')
    })
  }

  return nodes
}

const astNodes = computed(() => parseMarkdownAst(props.content))

function formatLangBadge(lang?: string): string {
  const l = (lang || 'text').toLowerCase()
  const map: Record<string, string> = {
    text: 'TEXT · 结构示意',
    txt: 'TEXT · 纯文本',
    plaintext: 'TEXT',
    js: 'JavaScript',
    javascript: 'JavaScript',
    ts: 'TypeScript',
    typescript: 'TypeScript',
    py: 'Python',
    python: 'Python',
    sh: 'Shell / Bash',
    bash: 'Bash',
    zsh: 'Zsh',
    json: 'JSON',
    sql: 'SQL',
    html: 'HTML',
    css: 'CSS',
    rust: 'Rust',
    rs: 'Rust',
    go: 'Go',
    java: 'Java',
    yaml: 'YAML',
    yml: 'YAML'
  }
  return map[l] || l.toUpperCase()
}

function shouldShowLineNumbers(code = '', lang = ''): boolean {
  const lines = code.split('\n')
  if (lines.length < 3) return false
  const l = lang.toLowerCase()
  if (l === 'text' || l === 'txt' || l === 'plaintext' || /[├└│─]/.test(code)) {
    return false
  }
  return true
}

function handleCopyCode(code: string, lang: string, idx: number) {
  copiedBlockIdx.value = idx
  setTimeout(() => {
    if (copiedBlockIdx.value === idx) copiedBlockIdx.value = null
  }, 1500)
  emit('copy-code', { code, lang })
}
</script>

<template>
  <div class="ai-markdown-body space-y-3 text-[12.5px] leading-[1.7] break-words">
    <template v-for="(node, idx) in astNodes" :key="idx">
      <!-- 1. Headings (`h1` ~ `h4`) -->
      <div
        v-if="node.type === 'heading'"
        :class="[
          node.level === 1
            ? 'text-[15px] font-bold pt-2 pb-1 border-b ' +
              (isDark ? 'text-white border-slate-800' : 'text-slate-900 border-slate-200/80')
            : node.level === 2
              ? 'text-[13.5px] font-bold pt-2 flex items-center gap-2 ' +
                (isDark ? 'text-indigo-300' : 'text-slate-900')
              : 'text-[12.5px] font-bold pt-1 ' +
                (isDark ? 'text-slate-200' : 'text-slate-800')
        ]"
      >
        <span
          v-if="node.level === 2"
          class="w-1 h-3.5 rounded-full bg-indigo-500 shrink-0"
        ></span>
        <span v-html="renderInlineMarkdown(node.text || '', isDark)"></span>
      </div>

      <!-- 2. Horizontal Rule (`---`) -->
      <hr
        v-else-if="node.type === 'hr'"
        class="my-2 border-0 h-px"
        :class="
          isDark
            ? 'bg-gradient-to-r from-transparent via-slate-800 to-transparent'
            : 'bg-gradient-to-r from-transparent via-slate-200 to-transparent'
        "
      />

      <!-- 3. Blockquote / Callout (`> ...`) -->
      <blockquote
        v-else-if="node.type === 'blockquote'"
        class="pl-3.5 pr-3 py-2 rounded-r-xl border-l-[3px] border-indigo-500 text-[12px] leading-relaxed"
        :class="
          isDark
            ? 'bg-indigo-500/10 text-indigo-200/95'
            : 'bg-indigo-50/75 text-slate-700'
        "
      >
        <div
          class="whitespace-pre-wrap"
          v-html="renderInlineMarkdown(node.text || '', isDark)"
        ></div>
      </blockquote>

      <!-- 4. Unordered List (`- ...`) -->
      <ul
        v-else-if="node.type === 'ul'"
        class="space-y-1.5 pl-1"
      >
        <li
          v-for="(item, iIdx) in node.items"
          :key="iIdx"
          class="flex items-start gap-2 leading-relaxed"
        >
          <span
            class="w-1.5 h-1.5 rounded-full mt-[8px] shrink-0"
            :class="isDark ? 'bg-indigo-400' : 'bg-indigo-500'"
          ></span>
          <span
            class="flex-1 min-w-0"
            v-html="renderInlineMarkdown(item, isDark)"
          ></span>
        </li>
      </ul>

      <!-- 5. Ordered List (`1. ...`) -->
      <ol
        v-else-if="node.type === 'ol'"
        class="space-y-1.5 pl-1"
      >
        <li
          v-for="(item, iIdx) in node.items"
          :key="iIdx"
          class="flex items-start gap-2 leading-relaxed"
        >
          <span
            class="inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 rounded-md text-[10.5px] font-mono font-bold mt-[2px] shrink-0"
            :class="
              isDark
                ? 'bg-indigo-500/20 text-indigo-300'
                : 'bg-indigo-50 text-indigo-600'
            "
          >
            {{ iIdx + 1 }}
          </span>
          <span
            class="flex-1 min-w-0"
            v-html="renderInlineMarkdown(item, isDark)"
          ></span>
        </li>
      </ol>

      <!-- 6. GFM Structured Table (`| col | col |`) -->
      <div
        v-else-if="node.type === 'table'"
        class="my-2 rounded-xl border overflow-x-auto"
        :class="isDark ? 'border-slate-800 bg-slate-950/40' : 'border-slate-200/90 bg-white'"
      >
        <table class="w-full border-collapse text-left text-[12px]">
          <thead>
            <tr
              class="border-b"
              :class="
                isDark
                  ? 'bg-slate-900/90 border-slate-800 text-slate-200'
                  : 'bg-slate-50 border-slate-200/90 text-slate-700'
              "
            >
              <th
                v-for="(hdr, hIdx) in node.headers"
                :key="hIdx"
                class="px-3 py-2 font-semibold whitespace-nowrap"
                :class="[
                  node.aligns?.[hIdx] === 'center'
                    ? 'text-center'
                    : node.aligns?.[hIdx] === 'right'
                      ? 'text-right'
                      : 'text-left'
                ]"
                v-html="renderInlineMarkdown(hdr, isDark)"
              ></th>
            </tr>
          </thead>
          <tbody class="divide-y" :class="isDark ? 'divide-slate-800/70' : 'divide-slate-200/60'">
            <tr
              v-for="(row, rIdx) in node.rows"
              :key="rIdx"
              class="transition-colors"
              :class="
                isDark
                  ? 'hover:bg-slate-900/50 text-slate-300'
                  : 'hover:bg-indigo-50/35 text-slate-700'
              "
            >
              <td
                v-for="(cell, cIdx) in row"
                :key="cIdx"
                class="px-3 py-2 leading-snug"
                :class="[
                  cIdx === 0 ? 'font-medium whitespace-nowrap' : '',
                  node.aligns?.[cIdx] === 'center'
                    ? 'text-center'
                    : node.aligns?.[cIdx] === 'right'
                      ? 'text-right'
                      : 'text-left'
                ]"
                v-html="renderInlineMarkdown(cell, isDark)"
              ></td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- 7. Adaptive Light/Dark Syntax-Highlighted Code Block -->
      <div
        v-else-if="node.type === 'code'"
        class="my-2 rounded-xl border overflow-hidden transition-colors"
        :class="
          isDark
            ? 'bg-[#0D1117] border-slate-800/90 text-slate-100'
            : 'bg-[#F8FAFC] border-slate-200/90 text-slate-800 shadow-[0_1px_2px_rgba(15,23,42,0.03)]'
        "
      >
        <!-- Code Block Header Toolbar -->
        <div
          class="px-3 py-1.5 border-b flex items-center justify-between text-[11px] select-none"
          :class="
            isDark
              ? 'bg-[#161B22] border-slate-800 text-slate-400'
              : 'bg-[#EEF2F6]/90 border-slate-200/85 text-slate-600'
          "
        >
          <div class="flex items-center gap-2">
            <span class="flex items-center gap-1">
              <span class="w-2 h-2 rounded-full bg-indigo-500/70"></span>
              <span
                class="font-mono font-bold tracking-tight"
                :class="isDark ? 'text-indigo-300' : 'text-indigo-700'"
              >
                {{ formatLangBadge(node.lang) }}
              </span>
            </span>
            <span class="text-[10px] font-mono opacity-65">
              {{ (node.code || '').split('\n').length }} 行
            </span>
          </div>

          <div class="flex items-center gap-2.5">
            <button
              v-if="node.lang === 'json'"
              type="button"
              class="hover:text-indigo-500 font-medium cursor-pointer transition-colors"
              @click="emit('send-to-plugin', { code: node.code || '', target: 'json-studio' })"
            >
              送入 JSON 助手
            </button>
            <button
              type="button"
              class="hover:text-indigo-500 cursor-pointer transition-colors"
              :title="wrapBlockMap[idx] ? '切换为横向滚动' : '切换为自动折行'"
              @click="wrapBlockMap[idx] = !wrapBlockMap[idx]"
            >
              {{ wrapBlockMap[idx] ? '取消折行' : '自动折行' }}
            </button>
            <button
              type="button"
              class="flex items-center gap-1 font-semibold cursor-pointer transition-colors"
              :class="
                copiedBlockIdx === idx
                  ? 'text-emerald-500'
                  : isDark
                    ? 'text-indigo-400 hover:text-indigo-300'
                    : 'text-indigo-600 hover:text-indigo-700'
              "
              @click="handleCopyCode(node.code || '', node.lang || 'code', idx)"
            >
              <IconRenderer
                :name="copiedBlockIdx === idx ? 'Check' : 'Copy'"
                :size="11"
              />
              <span>{{ copiedBlockIdx === idx ? '已复制' : '复制代码' }}</span>
            </button>
          </div>
        </div>

        <!-- Code Body with Optional Line Numbers -->
        <div class="flex overflow-x-auto">
          <div
            v-if="shouldShowLineNumbers(node.code, node.lang)"
            aria-hidden="true"
            class="py-3 pl-3 pr-2 select-none text-right font-mono text-[11px] leading-[1.65] border-r shrink-0"
            :class="
              isDark
                ? 'bg-[#090D14] border-slate-800/70 text-slate-600'
                : 'bg-slate-100/70 border-slate-200/70 text-slate-400'
            "
          >
            <div
              v-for="lineNum in (node.code || '').split('\n').length"
              :key="lineNum"
            >
              {{ lineNum }}
            </div>
          </div>
          <pre
            class="flex-1 p-3 font-mono text-[11.5px] leading-[1.65] m-0"
            :class="wrapBlockMap[idx] ? 'whitespace-pre-wrap break-all' : 'overflow-x-auto'"
          ><code v-html="highlightCodeHtml(node.code || '', node.lang || 'text', isDark)"></code></pre>
        </div>
      </div>

      <!-- 8. Regular Paragraph -->
      <p
        v-else
        class="whitespace-pre-wrap leading-[1.72]"
        v-html="renderInlineMarkdown(node.text || '', isDark)"
      ></p>
    </template>

    <!-- Streaming Cursor Indicator -->
    <span
      v-if="streaming"
      class="inline-block w-1.5 h-4 ml-1 align-middle rounded-xs bg-indigo-500 animate-pulse"
    ></span>
  </div>
</template>
