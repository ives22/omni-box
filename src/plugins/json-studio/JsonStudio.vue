<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import { kvGet, kvPut } from '@/bridge/backend'
import IconRenderer from '@/components/IconRenderer.vue'

type OutputMode = 'json' | 'minified' | 'xml' | 'yaml' | 'escaped' | 'struct'
type IndentOption = 2 | 4 | 'tab'
type SortOrder = 'asc' | 'desc'

interface JsonHistoryEntry {
  id: string
  title: string
  content: string
  savedAt: string
}

interface RenderLine {
  lineNo: number
  indentLevel: number
  path: string
  collapsible: boolean
  collapsed: boolean
  keyName?: string
  openBracket?: string
  closeBracket?: string
  collapsedSummary?: string
  valueType?: 'string' | 'number' | 'boolean' | 'null' | 'raw'
  valueText?: string
  trailingComma: boolean
  searchTextRaw: string
  searchTextLower: string
}

const store = useLauncherStore()

const DEFAULT_JSON = JSON.stringify(
  {
    project: 'OmniBox JSON Workbench',
    features: ['format', 'convert', 'inspect'],
    local: true,
    engine: {
      runtime: 'Tauri v2 + Rust',
      latencyMs: 0.15,
      sandboxIsolated: true
    }
  },
  null,
  2
)

const rawInput = ref(DEFAULT_JSON)
const outputMode = ref<OutputMode>('json')
const indentOption = ref<IndentOption>(2)
const sortOrder = ref<SortOrder>('asc')
const structLang = ref<'ts' | 'rust' | 'go'>('ts')

// Collapsible paths state for Left (when in tree view) & Right JSON viewer
const leftFoldedPaths = ref<Set<string>>(new Set())
const rightFoldedPaths = ref<Set<string>>(new Set())
const leftViewMode = ref<'edit' | 'fold'>('edit')

// Saved History Drawer
const showHistoryModal = ref(false)
const historyList = ref<JsonHistoryEntry[]>([])

// Content Search & Line Filter Bar (Triggered by Ctrl+F / Command+F or Search Button)
const showSearchBar = ref(false)
// Decouple raw input typing from heavy search evaluation to guarantee 0ms keystroke latency
const searchInputText = ref('')
const searchKeyword = ref('')
let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null
const caseSensitive = ref(false)
const filterMatchedOnly = ref(false)
const currentMatchIdx = ref(0)
const searchInputRef = ref<HTMLInputElement | null>(null)
const rightScrollContainerRef = ref<HTMLElement | null>(null)
const rightRawScrollContainerRef = ref<HTMLElement | null>(null)

// Virtual Scrolling state for Large JSON & Raw Text rendering (> 250 lines)
const VIRTUAL_THRESHOLD = 250
const VIRTUAL_ROW_HEIGHT = 20
const VIRTUAL_OVERSCAN = 30
const rightScrollTop = ref(0)
const rightViewportHeight = ref(650)
const rightRawScrollTop = ref(0)
const rightRawViewportHeight = ref(650)

const indentUnit = computed(() => {
  if (indentOption.value === 'tab') return '\t'
  return indentOption.value
})

const indentSpacesString = computed(() => {
  if (indentOption.value === 'tab') return '    '
  return ' '.repeat(indentOption.value)
})

const parsedResult = computed<{ ok: boolean; data: unknown; error: string | null }>(() => {
  const trimmed = rawInput.value.trim()
  if (!trimmed) return { ok: true, data: null, error: null }
  try {
    return { ok: true, data: JSON.parse(trimmed), error: null }
  } catch (e) {
    return { ok: false, data: null, error: (e as Error).message }
  }
})

// Apply JSONPath / dot-path filter from top SubInput if user types a path
const activeData = computed<unknown>(() => {
  if (!parsedResult.value.ok || parsedResult.value.data === null) return null
  const query = store.subInputQuery.trim()
  if (!query) return parsedResult.value.data

  try {
    const cleanPath = query.replace(/^\$\.?/, '').replace(/^\./, '')
    if (!cleanPath) return parsedResult.value.data

    const segments = cleanPath
      .replace(/\[(\d+)\]/g, '.$1')
      .split('.')
      .filter(Boolean)

    let current: unknown = parsedResult.value.data
    for (const seg of segments) {
      if (
        current !== null &&
        typeof current === 'object' &&
        seg in (current as Record<string, unknown>)
      ) {
        current = (current as Record<string, unknown>)[seg]
      } else {
        return { _notice: `路径 "${query}" 未匹配到节点` }
      }
    }
    return current
  } catch {
    return parsedResult.value.data
  }
})

// ============================================================================
// 1. Key Sorting (A-Z Ascending & Z-A Descending)
// ============================================================================
function sortKeysDeep(val: unknown, order: SortOrder = 'asc'): unknown {
  if (Array.isArray(val)) return val.map((item) => sortKeysDeep(item, order))
  if (val !== null && typeof val === 'object') {
    const sorted: Record<string, unknown> = {}
    const keys = Object.keys(val as Record<string, unknown>).sort((a, b) =>
      order === 'asc' ? a.localeCompare(b) : b.localeCompare(a)
    )
    for (const k of keys) {
      sorted[k] = sortKeysDeep((val as Record<string, unknown>)[k], order)
    }
    return sorted
  }
  return val
}

// ============================================================================
// 2. JSON -> XML Converter
// ============================================================================
function escapeXml(unsafe: string): string {
  return unsafe
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;')
}

function sanitizeXmlTag(tag: string): string {
  const cleaned = tag.replace(/[^a-zA-Z0-9_.-]/g, '_')
  return /^[a-zA-Z_]/.test(cleaned) ? cleaned : `node_${cleaned || 'item'}`
}

function jsonToXmlNodes(val: unknown, tagName: string, depth: number, unit: string): string {
  const pad = unit.repeat(depth)
  const safeTag = sanitizeXmlTag(tagName)

  if (val === null || val === undefined) {
    return `${pad}<${safeTag} xsi:nil="true" />`
  }
  if (typeof val === 'boolean' || typeof val === 'number') {
    return `${pad}<${safeTag}>${String(val)}</${safeTag}>`
  }
  if (typeof val === 'string') {
    return `${pad}<${safeTag}>${escapeXml(val)}</${safeTag}>`
  }
  if (Array.isArray(val)) {
    if (val.length === 0) return `${pad}<${safeTag} />`
    return val
      .map((item) => jsonToXmlNodes(item, safeTag === 'root' ? 'item' : safeTag, depth, unit))
      .join('\n')
  }
  if (typeof val === 'object') {
    const entries = Object.entries(val as Record<string, unknown>)
    if (entries.length === 0) return `${pad}<${safeTag} />`
    const children = entries
      .map(([k, v]) => jsonToXmlNodes(v, k, depth + 1, unit))
      .join('\n')
    return `${pad}<${safeTag}>\n${children}\n${pad}</${safeTag}>`
  }
  return `${pad}<${safeTag}>${escapeXml(String(val))}</${safeTag}>`
}

function convertJsonToXml(val: unknown, unit: string): string {
  const header = '<?xml version="1.0" encoding="UTF-8"?>'
  const body = jsonToXmlNodes(val, 'root', 0, unit)
  return `${header}\n${body}`
}

// ============================================================================
// 3. JSON -> YAML Converter
// ============================================================================
function formatYamlScalar(val: unknown): string {
  if (val === null || val === undefined) return 'null'
  if (typeof val === 'boolean' || typeof val === 'number') return String(val)
  const str = String(val)
  if (
    str === '' ||
    /[:#{}[\],&*!|>'"%@`]/.test(str) ||
    /^(true|false|null|yes|no|on|off)$/i.test(str) ||
    /^\s|\s$/.test(str) ||
    str.includes('\n')
  ) {
    return JSON.stringify(str)
  }
  return str
}

function convertJsonToYaml(val: unknown, depth = 0, unit = '  '): string {
  const pad = unit.repeat(depth)
  if (val === null || typeof val !== 'object') {
    return `${pad}${formatYamlScalar(val)}`
  }
  if (Array.isArray(val)) {
    if (val.length === 0) return `${pad}[]`
    return val
      .map((item) => {
        if (item !== null && typeof item === 'object') {
          const nested = convertJsonToYaml(item, depth + 1, unit)
          const trimmedFirst = nested.trimStart()
          return `${pad}- ${trimmedFirst}`
        }
        return `${pad}- ${formatYamlScalar(item)}`
      })
      .join('\n')
  }
  const entries = Object.entries(val as Record<string, unknown>)
  if (entries.length === 0) return `${pad}{}`
  return entries
    .map(([k, v]) => {
      const safeKey = /[:#{}[\],&*!|>'"%@`\s]/.test(k) ? JSON.stringify(k) : k
      if (v !== null && typeof v === 'object') {
        if (Array.isArray(v) && v.length === 0) return `${pad}${safeKey}: []`
        if (!Array.isArray(v) && Object.keys(v as object).length === 0) return `${pad}${safeKey}: {}`
        return `${pad}${safeKey}:\n${convertJsonToYaml(v, depth + 1, unit)}`
      }
      return `${pad}${safeKey}: ${formatYamlScalar(v)}`
    })
    .join('\n')
}

// ============================================================================
// 4. Type Struct Generator (TS / Rust / Go)
// ============================================================================
const generatedStructCode = computed(() => {
  if (!parsedResult.value.ok || !activeData.value || typeof activeData.value !== 'object') {
    return '// 请输入合法的 JSON 对象以生成类型定义'
  }
  const obj = Array.isArray(activeData.value)
    ? activeData.value[0] || {}
    : activeData.value

  const entries = Object.entries(obj as Record<string, unknown>)

  if (structLang.value === 'ts') {
    const lines = entries.map(([k, v]) => {
      const t = Array.isArray(v) ? 'unknown[]' : v === null ? 'null' : typeof v
      return `  ${k}: ${t};`
    })
    return `export interface RootObject {\n${lines.join('\n')}\n}`
  }

  if (structLang.value === 'rust') {
    const lines = entries.map(([k, v]) => {
      let t = 'serde_json::Value'
      if (typeof v === 'string') t = 'String'
      else if (typeof v === 'boolean') t = 'bool'
      else if (typeof v === 'number') t = Number.isInteger(v) ? 'i64' : 'f64'
      else if (Array.isArray(v)) t = 'Vec<serde_json::Value>'
      return `    pub ${k}: ${t},`
    })
    return `use serde::{Deserialize, Serialize};\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct RootObject {\n${lines.join('\n')}\n}`
  }

  const lines = entries.map(([k, v]) => {
    const capKey = k.charAt(0).toUpperCase() + k.slice(1)
    let t = 'interface{}'
    if (typeof v === 'string') t = 'string'
    else if (typeof v === 'boolean') t = 'bool'
    else if (typeof v === 'number') t = Number.isInteger(v) ? 'int64' : 'float64'
    else if (Array.isArray(v)) t = '[]interface{}'
    return `\t${capKey} ${t} \`json:"${k}"\``
  })
  return `type RootObject struct {\n${lines.join('\n')}\n}`
})

// ============================================================================
// 5. Output Text & Collapsible JSON Line Builder
// ============================================================================
const formattedOutputText = computed<string>(() => {
  if (!parsedResult.value.ok) {
    return `// JSON 语法错误：\n// ${parsedResult.value.error}`
  }
  if (activeData.value === null) {
    return ''
  }
  const unitStr = indentOption.value === 'tab' ? '\t' : ' '.repeat(indentOption.value)

  switch (outputMode.value) {
    case 'minified':
      return JSON.stringify(activeData.value)
    case 'xml':
      return convertJsonToXml(activeData.value, unitStr)
    case 'yaml':
      return convertJsonToYaml(
        activeData.value,
        0,
        indentOption.value === 'tab' ? '  ' : ' '.repeat(indentOption.value)
      )
    case 'escaped':
      return JSON.stringify(JSON.stringify(activeData.value))
    case 'struct':
      return generatedStructCode.value
    case 'json':
    default:
      return JSON.stringify(activeData.value, null, indentUnit.value)
  }
})

function buildCollapsibleJsonLines(
  val: unknown,
  foldedSet: Set<string>,
  path = '$',
  indentLevel = 0,
  keyName: string | undefined = undefined,
  trailingComma = false,
  lines: RenderLine[] = []
): RenderLine[] {
  const pushLine = (partial: Omit<RenderLine, 'lineNo' | 'searchTextRaw' | 'searchTextLower'>) => {
    let raw = ''
    if (partial.keyName !== undefined) {
      raw += `"${partial.keyName}": `
    }
    if (partial.openBracket) raw += partial.openBracket
    if (partial.valueText !== undefined) raw += partial.valueText
    if (partial.closeBracket) raw += partial.closeBracket
    lines.push({
      lineNo: lines.length + 1,
      ...partial,
      searchTextRaw: raw,
      searchTextLower: raw.toLowerCase()
    })
  }

  if (val !== null && typeof val === 'object') {
    const isArr = Array.isArray(val)
    const entries = isArr
      ? (val as unknown[]).map((v, i) => [String(i), v] as const)
      : Object.entries(val as Record<string, unknown>)
    const openB = isArr ? '[' : '{'
    const closeB = isArr ? ']' : '}'

    if (entries.length === 0) {
      pushLine({
        indentLevel,
        path,
        collapsible: false,
        collapsed: false,
        keyName,
        valueType: 'raw',
        valueText: `${openB}${closeB}`,
        trailingComma
      })
      return lines
    }

    const isFolded = foldedSet.has(path)
    if (isFolded) {
      pushLine({
        indentLevel,
        path,
        collapsible: true,
        collapsed: true,
        keyName,
        openBracket: openB,
        closeBracket: closeB,
        collapsedSummary: isArr ? `${entries.length} items` : `${entries.length} keys`,
        trailingComma
      })
      return lines
    }

    pushLine({
      indentLevel,
      path,
      collapsible: true,
      collapsed: false,
      keyName,
      openBracket: openB,
      trailingComma: false
    })

    entries.forEach(([k, childVal], idx) => {
      const childPath = isArr ? `${path}[${k}]` : `${path}.${k}`
      const isLast = idx === entries.length - 1
      buildCollapsibleJsonLines(
        childVal,
        foldedSet,
        childPath,
        indentLevel + 1,
        isArr ? undefined : k,
        !isLast,
        lines
      )
    })

    pushLine({
      indentLevel,
      path: `${path}:close`,
      collapsible: false,
      collapsed: false,
      closeBracket: closeB,
      trailingComma
    })
    return lines
  }

  let vType: RenderLine['valueType'] = 'string'
  let vText = ''
  if (val === null) {
    vType = 'null'
    vText = 'null'
  } else if (typeof val === 'boolean') {
    vType = 'boolean'
    vText = String(val)
  } else if (typeof val === 'number') {
    vType = 'number'
    vText = String(val)
  } else {
    vType = 'string'
    vText = JSON.stringify(String(val))
  }

  pushLine({
    indentLevel,
    path,
    collapsible: false,
    collapsed: false,
    keyName,
    valueType: vType,
    valueText: vText,
    trailingComma
  })

  return lines
}

function collectAllCollapsiblePaths(val: unknown, path = '$', out: string[] = []): string[] {
  if (val !== null && typeof val === 'object') {
    const isArr = Array.isArray(val)
    const entries = isArr
      ? (val as unknown[]).map((v, i) => [String(i), v] as const)
      : Object.entries(val as Record<string, unknown>)
    if (entries.length > 0) {
      out.push(path)
      entries.forEach(([k, childVal]) => {
        const childPath = isArr ? `${path}[${k}]` : `${path}.${k}`
        collectAllCollapsiblePaths(childVal, childPath, out)
      })
    }
  }
  return out
}

const rightJsonLines = computed<RenderLine[]>(() => {
  if (!parsedResult.value.ok || activeData.value === null || outputMode.value !== 'json') {
    return []
  }
  return buildCollapsibleJsonLines(activeData.value, rightFoldedPaths.value)
})

const leftJsonLines = computed<RenderLine[]>(() => {
  if (!parsedResult.value.ok || parsedResult.value.data === null) {
    return []
  }
  return buildCollapsibleJsonLines(parsedResult.value.data, leftFoldedPaths.value)
})

const inputLineNumbers = computed(() => {
  const count = Math.max(1, rawInput.value.split('\n').length)
  return Array.from({ length: count }, (_, i) => i + 1)
})

const outputLinesArray = computed(() => {
  if (!formattedOutputText.value) return ['']
  return formattedOutputText.value.split('\n')
})

const inputStats = computed(() => {
  const chars = rawInput.value.length
  const lines = rawInput.value ? rawInput.value.split('\n').length : 0
  return `${chars} 字符 · ${lines} 行`
})

// ============================================================================
// Content Search & Filtering Logic (Ctrl+F / Command+F, High-Performance)
// ============================================================================
watch(searchInputText, (val) => {
  if (searchDebounceTimer) {
    clearTimeout(searchDebounceTimer)
    searchDebounceTimer = null
  }
  if (!val.trim()) {
    searchKeyword.value = ''
    return
  }
  searchDebounceTimer = setTimeout(() => {
    searchKeyword.value = val
  }, 45)
})

function splitHighlightParts(text: string): { text: string; highlight: boolean }[] {
  const kw = searchKeyword.value.trim()
  if (!kw || !text) return [{ text, highlight: false }]

  const source = caseSensitive.value ? text : text.toLowerCase()
  const needle = caseSensitive.value ? kw : kw.toLowerCase()
  let idx = source.indexOf(needle, 0)
  if (idx === -1) {
    return [{ text, highlight: false }]
  }

  const parts: { text: string; highlight: boolean }[] = []
  let start = 0
  while (idx !== -1) {
    if (idx > start) {
      parts.push({ text: text.slice(start, idx), highlight: false })
    }
    parts.push({ text: text.slice(idx, idx + kw.length), highlight: true })
    start = idx + kw.length
    idx = source.indexOf(needle, start)
  }
  if (start < text.length) {
    parts.push({ text: text.slice(start), highlight: false })
  }
  return parts
}

const rawLineEntries = computed<{ lineNo: number; text: string; textLower: string }[]>(() => {
  return outputLinesArray.value.map((text, idx) => ({
    lineNo: idx + 1,
    text,
    textLower: text.toLowerCase()
  }))
})

const matchedJsonLineNos = computed<number[]>(() => {
  const kw = searchKeyword.value.trim()
  if (!kw) return []
  const isCase = caseSensitive.value
  const needle = isCase ? kw : kw.toLowerCase()
  const lines = rightJsonLines.value
  const out: number[] = []
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!
    const hay = isCase ? line.searchTextRaw : line.searchTextLower
    if (hay.includes(needle)) {
      out.push(line.lineNo)
    }
  }
  return out
})

const matchedRawLineNos = computed<number[]>(() => {
  const kw = searchKeyword.value.trim()
  if (!kw) return []
  const isCase = caseSensitive.value
  const needle = isCase ? kw : kw.toLowerCase()
  const entries = rawLineEntries.value
  const out: number[] = []
  for (let i = 0; i < entries.length; i++) {
    const item = entries[i]!
    const hay = isCase ? item.text : item.textLower
    if (hay.includes(needle)) {
      out.push(item.lineNo)
    }
  }
  return out
})

const activeMatchedLineNos = computed<number[]>(() => {
  if (outputMode.value === 'json' && parsedResult.value.ok && rightJsonLines.value.length > 0) {
    return matchedJsonLineNos.value
  }
  return matchedRawLineNos.value
})

const matchedLineNoSet = computed<Set<number>>(() => {
  return new Set(activeMatchedLineNos.value)
})

const currentMatchedLineNo = computed<number | null>(() => {
  if (activeMatchedLineNos.value.length === 0) return null
  const safeIdx = Math.min(currentMatchIdx.value, activeMatchedLineNos.value.length - 1)
  return activeMatchedLineNos.value[safeIdx] ?? null
})

const displayedRightJsonLines = computed<RenderLine[]>(() => {
  if (filterMatchedOnly.value && searchKeyword.value.trim()) {
    const set = matchedLineNoSet.value
    return rightJsonLines.value.filter((line) => set.has(line.lineNo))
  }
  return rightJsonLines.value
})

const displayedRawLines = computed<{ lineNo: number; text: string }[]>(() => {
  if (filterMatchedOnly.value && searchKeyword.value.trim()) {
    const set = matchedLineNoSet.value
    return rawLineEntries.value.filter((item) => set.has(item.lineNo))
  }
  return rawLineEntries.value
})

// Virtual Windowing for JSON Tree & Raw View when line count exceeds VIRTUAL_THRESHOLD
const isJsonVirtualActive = computed(() => displayedRightJsonLines.value.length > VIRTUAL_THRESHOLD)

const virtualRightJsonWindow = computed(() => {
  const all = displayedRightJsonLines.value
  if (!isJsonVirtualActive.value) {
    return { items: all, topPad: 0, bottomPad: 0 }
  }
  const total = all.length
  const firstVisible = Math.floor(rightScrollTop.value / VIRTUAL_ROW_HEIGHT)
  const visibleCount = Math.ceil(rightViewportHeight.value / VIRTUAL_ROW_HEIGHT)
  const start = Math.max(0, firstVisible - VIRTUAL_OVERSCAN)
  const end = Math.min(total, firstVisible + visibleCount + VIRTUAL_OVERSCAN)
  return {
    items: all.slice(start, end),
    topPad: start * VIRTUAL_ROW_HEIGHT,
    bottomPad: Math.max(0, (total - end) * VIRTUAL_ROW_HEIGHT)
  }
})

const isRawVirtualActive = computed(() => displayedRawLines.value.length > VIRTUAL_THRESHOLD)

const virtualRawWindow = computed(() => {
  const all = displayedRawLines.value
  if (!isRawVirtualActive.value) {
    return { items: all, topPad: 0, bottomPad: 0 }
  }
  const total = all.length
  const firstVisible = Math.floor(rightRawScrollTop.value / VIRTUAL_ROW_HEIGHT)
  const visibleCount = Math.ceil(rightRawViewportHeight.value / VIRTUAL_ROW_HEIGHT)
  const start = Math.max(0, firstVisible - VIRTUAL_OVERSCAN)
  const end = Math.min(total, firstVisible + visibleCount + VIRTUAL_OVERSCAN)
  return {
    items: all.slice(start, end),
    topPad: start * VIRTUAL_ROW_HEIGHT,
    bottomPad: Math.max(0, (total - end) * VIRTUAL_ROW_HEIGHT)
  }
})

function handleRightTreeScroll(e: Event) {
  const el = e.target as HTMLElement
  rightScrollTop.value = el.scrollTop
  if (el.clientHeight > 0) {
    rightViewportHeight.value = el.clientHeight
  }
}

function handleRightRawScroll(e: Event) {
  const el = e.target as HTMLElement
  rightRawScrollTop.value = el.scrollTop
  if (el.clientHeight > 0) {
    rightRawViewportHeight.value = el.clientHeight
  }
}

function scrollToActiveMatch() {
  const lineNo = currentMatchedLineNo.value
  if (lineNo === null) return

  const isJsonTree =
    outputMode.value === 'json' && parsedResult.value.ok && rightJsonLines.value.length > 0
  const container = isJsonTree ? rightScrollContainerRef.value : rightRawScrollContainerRef.value
  if (!container) return

  // Compute exact row index in displayed list for O(1)-style instant scroll positioning
  const list = isJsonTree ? displayedRightJsonLines.value : displayedRawLines.value
  const rowIndex = list.findIndex((item) => item.lineNo === lineNo)
  if (rowIndex !== -1) {
    const viewH = container.clientHeight || 500
    const targetTop = Math.max(0, rowIndex * VIRTUAL_ROW_HEIGHT - viewH / 2 + VIRTUAL_ROW_HEIGHT)
    container.scrollTop = targetTop
    if (isJsonTree) {
      rightScrollTop.value = targetTop
    } else {
      rightRawScrollTop.value = targetTop
    }
  }

  nextTick(() => {
    const targetEl = container.querySelector(`[data-right-line="${lineNo}"]`) as HTMLElement | null
    if (targetEl) {
      targetEl.scrollIntoView({ block: 'center', behavior: 'instant' as ScrollBehavior })
    }
  })
}

function flushSearchInputNow() {
  if (searchDebounceTimer) {
    clearTimeout(searchDebounceTimer)
    searchDebounceTimer = null
  }
  if (searchKeyword.value !== searchInputText.value) {
    searchKeyword.value = searchInputText.value
  }
}

function gotoNextMatch() {
  flushSearchInputNow()
  const total = activeMatchedLineNos.value.length
  if (total === 0) return
  currentMatchIdx.value = (currentMatchIdx.value + 1) % total
  scrollToActiveMatch()
}

function gotoPrevMatch() {
  flushSearchInputNow()
  const total = activeMatchedLineNos.value.length
  if (total === 0) return
  currentMatchIdx.value = (currentMatchIdx.value - 1 + total) % total
  scrollToActiveMatch()
}

function openSearchBar() {
  showSearchBar.value = true
  const selectedText = window.getSelection()?.toString().trim()
  if (selectedText && selectedText.length <= 64 && !selectedText.includes('\n')) {
    searchInputText.value = selectedText
    searchKeyword.value = selectedText
  }
  nextTick(() => {
    searchInputRef.value?.focus()
    searchInputRef.value?.select()
  })
}

function closeSearchBar() {
  if (searchDebounceTimer) {
    clearTimeout(searchDebounceTimer)
    searchDebounceTimer = null
  }
  showSearchBar.value = false
  searchInputText.value = ''
  searchKeyword.value = ''
  filterMatchedOnly.value = false
  currentMatchIdx.value = 0
}

function handleSearchInputKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    if (e.shiftKey) {
      gotoPrevMatch()
    } else {
      gotoNextMatch()
    }
  } else if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    closeSearchBar()
  }
}

watch([searchKeyword, caseSensitive], ([newKw]) => {
  currentMatchIdx.value = 0
  if (newKw.trim() && rightFoldedPaths.value.size > 0) {
    // Automatically unfold collapsed nodes when searching so nested matches are never hidden
    rightFoldedPaths.value = new Set()
  }
  if (newKw.trim()) {
    scrollToActiveMatch()
  }
})

const outputStats = computed(() => {
  const text = formattedOutputText.value
  const chars = text.length
  const lines =
    outputMode.value === 'json' && rightJsonLines.value.length > 0
      ? displayedRightJsonLines.value.length
      : displayedRawLines.value.length
  if (filterMatchedOnly.value && searchKeyword.value.trim()) {
    return `已过滤 ${lines} 匹配行 · 共 ${chars} 字符`
  }
  return `${chars} 字符 · ${lines} 行`
})

const outputModeLabel = computed(() => {
  const map: Record<OutputMode, string> = {
    json: 'JSON',
    minified: 'MINIFIED',
    xml: 'XML',
    yaml: 'YAML',
    escaped: 'STRING',
    struct: structLang.value.toUpperCase()
  }
  return map[outputMode.value]
})

function toggleFoldPath(side: 'left' | 'right', path: string) {
  const targetRef = side === 'left' ? leftFoldedPaths : rightFoldedPaths
  const next = new Set(targetRef.value)
  if (next.has(path)) {
    next.delete(path)
  } else {
    next.add(path)
  }
  targetRef.value = next
}

function foldAll(side: 'left' | 'right') {
  if (!parsedResult.value.ok || !parsedResult.value.data) return
  if (side === 'left') {
    leftViewMode.value = 'fold'
  } else {
    outputMode.value = 'json'
  }
  const source = side === 'left' ? parsedResult.value.data : activeData.value
  const allPaths = collectAllCollapsiblePaths(source).filter((p) => p !== '$')
  if (side === 'left') {
    leftFoldedPaths.value = new Set(allPaths)
  } else {
    rightFoldedPaths.value = new Set(allPaths)
  }
}

function unfoldAll(side: 'left' | 'right') {
  if (side === 'left') {
    leftFoldedPaths.value = new Set()
  } else {
    rightFoldedPaths.value = new Set()
  }
}

// ============================================================================
// 6. Toolbar Actions (Matching JSON Lens Reference Screenshot)
// ============================================================================
function handleFormatAction() {
  if (!parsedResult.value.ok) {
    store.showToast(`JSON 语法错误: ${parsedResult.value.error}`, 'warning')
    return
  }
  outputMode.value = 'json'
  rawInput.value = JSON.stringify(parsedResult.value.data, null, indentUnit.value)
  store.showToast('已格式化并美化 JSON 结构')
}

function handleMinifyAction() {
  if (!parsedResult.value.ok) {
    store.showToast('JSON 语法有误，无法压缩', 'warning')
    return
  }
  outputMode.value = 'minified'
  rawInput.value = JSON.stringify(parsedResult.value.data)
  store.showToast('已移除空白并压缩为单行 JSON')
}

function handleValidateAction() {
  if (!rawInput.value.trim()) {
    store.showToast('请先输入需要校验的 JSON 内容', 'warning')
    return
  }
  if (parsedResult.value.ok) {
    store.showToast('校验通过：JSON 语法完全合法！')
  } else {
    store.showToast(`语法错误：${parsedResult.value.error}`, 'warning')
  }
}

function handleSortAction(order?: SortOrder) {
  if (order) {
    sortOrder.value = order
  }
  if (!parsedResult.value.ok) {
    store.showToast('JSON 语法有误，无法排序', 'warning')
    return
  }
  const sorted = sortKeysDeep(parsedResult.value.data, sortOrder.value)
  rawInput.value = JSON.stringify(sorted, null, indentUnit.value)
  outputMode.value = 'json'
  store.showToast(`已按属性键名 ${sortOrder.value === 'asc' ? '升序 (A-Z)' : '降序 (Z-A)'} 排序`)
}

function handleXmlConvert() {
  if (!parsedResult.value.ok) {
    store.showToast('JSON 语法有误，无法转换为 XML', 'warning')
    return
  }
  outputMode.value = 'xml'
  store.showToast('已转换为 XML 输出格式')
}

function handleYamlConvert() {
  if (!parsedResult.value.ok) {
    store.showToast('JSON 语法有误，无法转换为 YAML', 'warning')
    return
  }
  outputMode.value = 'yaml'
  store.showToast('已转换为 YAML 输出格式')
}

function handleEscapeAction() {
  if (!rawInput.value.trim()) return
  outputMode.value = 'escaped'
  rawInput.value = JSON.stringify(rawInput.value)
  store.showToast('已转义为 JSON 字符串')
}

function handleUnescapeAction() {
  try {
    const parsed = JSON.parse(rawInput.value)
    if (typeof parsed === 'string') {
      rawInput.value = parsed
      try {
        rawInput.value = JSON.stringify(JSON.parse(parsed), null, indentUnit.value)
        outputMode.value = 'json'
      } catch {
        // keep unescaped string
      }
      store.showToast('已成功反转义还原内容')
    } else {
      outputMode.value = 'json'
      rawInput.value = JSON.stringify(parsed, null, indentUnit.value)
      store.showToast('已还原格式化 JSON')
    }
  } catch {
    const manualUnescaped = rawInput.value
      .replace(/^"|"$/g, '')
      .replace(/\\"/g, '"')
      .replace(/\\n/g, '\n')
      .replace(/\\t/g, '\t')
      .replace(/\\\\/g, '\\')
    rawInput.value = manualUnescaped
    outputMode.value = 'json'
    store.showToast('已执行字符串反转义还原')
  }
}

watch(indentOption, () => {
  if (parsedResult.value.ok && parsedResult.value.data !== null && outputMode.value !== 'minified') {
    rawInput.value = JSON.stringify(parsedResult.value.data, null, indentUnit.value)
  }
})

async function copyText(text: string, label = '已复制到剪贴板') {
  await navigator.clipboard.writeText(text)
  store.showToast(label)
}

function downloadOutputFile() {
  const extMap: Record<OutputMode, string> = {
    json: 'json',
    minified: 'min.json',
    xml: 'xml',
    yaml: 'yaml',
    escaped: 'txt',
    struct: structLang.value === 'rust' ? 'rs' : structLang.value
  }
  const ext = extMap[outputMode.value] || 'json'
  const blob = new Blob([formattedOutputText.value], { type: 'text/plain;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `omnibox-export-${Date.now()}.${ext}`
  a.click()
  URL.revokeObjectURL(url)
  store.showToast(`已导出下载 .${ext} 文件`)
}

async function saveToHistory() {
  if (!rawInput.value.trim()) return
  const preview = rawInput.value.trim().replace(/\s+/g, ' ').slice(0, 38)
  const entry: JsonHistoryEntry = {
    id: `json-${Date.now()}`,
    title: preview,
    content: rawInput.value,
    savedAt: new Date().toLocaleTimeString()
  }
  historyList.value = [entry, ...historyList.value.slice(0, 19)]
  await kvPut('json-studio', 'history', historyList.value)
  store.showToast('已保存当前 JSON 快照至历史记录')
}

async function deleteHistoryEntry(id: string) {
  historyList.value = historyList.value.filter((h) => h.id !== id)
  await kvPut('json-studio', 'history', historyList.value)
}

function handlePluginKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'f') {
    e.preventDefault()
    e.stopPropagation()
    openSearchBar()
  }
}

onMounted(async () => {
  const winObj = window as unknown as {
    __omniboxPluginInterceptEscape?: () => boolean
  }
  winObj.__omniboxPluginInterceptEscape = () => {
    if (showSearchBar.value) {
      closeSearchBar()
      return true
    }
    if (showHistoryModal.value) {
      showHistoryModal.value = false
      return true
    }
    return false
  }
  window.addEventListener('keydown', handlePluginKeydown, true)

  const savedHist = await kvGet<JsonHistoryEntry[]>('json-studio', 'history', [])
  if (Array.isArray(savedHist)) {
    historyList.value = savedHist
  }

  if (store.pluginInitialPayload) {
    try {
      rawInput.value = JSON.stringify(JSON.parse(store.pluginInitialPayload), null, indentUnit.value)
    } catch {
      rawInput.value = store.pluginInitialPayload
    }
    store.pluginInitialPayload = ''
  } else {
    try {
      const savedDraft = localStorage.getItem('omnibox:live-draft:json-studio')
      if (savedDraft && savedDraft.trim()) {
        rawInput.value = savedDraft
      }
    } catch {
      // ignore
    }
  }
})

onUnmounted(() => {
  const winObj = window as unknown as {
    __omniboxPluginInterceptEscape?: () => boolean
  }
  if (winObj.__omniboxPluginInterceptEscape) {
    delete winObj.__omniboxPluginInterceptEscape
  }
  window.removeEventListener('keydown', handlePluginKeydown, true)
})

watch(rawInput, (val) => {
  try {
    localStorage.setItem('omnibox:live-draft:json-studio', val)
  } catch {
    // ignore
  }
})
</script>

<template>
  <div
    class="flex flex-col flex-1 h-full min-h-[500px] select-none overflow-hidden"
    :class="store.isDark ? 'bg-[#0E131F] text-slate-100' : 'bg-[#F8FAFC] text-slate-800'"
  >
    <!-- ======================================================================= -->
    <!-- Top Action Toolbar (Native OmniBox Compact Indigo Style, Zero Overflow) -->
    <!-- ======================================================================= -->
    <div
      class="flex items-center justify-between px-4 py-2 border-b text-xs shrink-0"
      :class="
        store.isDark
          ? 'bg-obsidian-950/60 border-slate-800/80'
          : 'bg-slate-50/90 border-slate-200/80'
      "
    >
      <div class="flex items-center gap-1.5 flex-wrap">
        <!-- 格式化 -->
        <button
          type="button"
          class="btn-primary !px-2.5 !py-1 !rounded-lg"
          @click="handleFormatAction"
        >
          <IconRenderer name="Wand2" :size="13" />
          <span>格式化</span>
        </button>

        <!-- 压缩 -->
        <button
          type="button"
          class="btn-secondary !px-2.5 !py-1 !rounded-lg"
          @click="handleMinifyAction"
        >
          压缩单行
        </button>

        <!-- Key 排序 (带 A-Z / Z-A 紧凑切换) -->
        <div
          class="inline-flex items-center rounded-lg border overflow-hidden text-xs"
          :class="
            store.isDark
              ? 'bg-slate-800/80 border-slate-700/80 text-slate-200'
              : 'bg-white border-slate-200/90 text-slate-700 shadow-[0_1px_2px_rgba(15,23,42,0.04)]'
          "
        >
          <button
            type="button"
            class="px-2 py-1 font-medium hover:text-indigo-500 cursor-pointer transition-colors"
            @click="handleSortAction()"
          >
            Key 排序
          </button>
          <div
            class="flex items-center border-l font-mono text-[10.5px]"
            :class="store.isDark ? 'border-slate-700/80' : 'border-slate-200/90'"
          >
            <button
              type="button"
              title="按 Key 升序排列 (A-Z)"
              class="px-1.5 py-1 cursor-pointer transition-colors"
              :class="
                sortOrder === 'asc'
                  ? store.isDark
                    ? 'bg-indigo-500/25 text-indigo-300 font-semibold'
                    : 'bg-indigo-50 text-indigo-600 font-semibold'
                  : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
              "
              @click="handleSortAction('asc')"
            >
              A-Z
            </button>
            <button
              type="button"
              title="按 Key 降序排列 (Z-A)"
              class="px-1.5 py-1 cursor-pointer transition-colors"
              :class="
                sortOrder === 'desc'
                  ? store.isDark
                    ? 'bg-indigo-500/25 text-indigo-300 font-semibold'
                    : 'bg-indigo-50 text-indigo-600 font-semibold'
                  : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
              "
              @click="handleSortAction('desc')"
            >
              Z-A
            </button>
          </div>
        </div>

        <!-- 转义 / 去转义 -->
        <button
          type="button"
          class="btn-secondary !px-2.5 !py-1 !rounded-lg"
          @click="handleEscapeAction"
        >
          转义
        </button>
        <button
          type="button"
          class="btn-secondary !px-2.5 !py-1 !rounded-lg"
          @click="handleUnescapeAction"
        >
          去转义
        </button>

        <div class="w-px h-4 mx-0.5" :class="store.isDark ? 'bg-slate-800' : 'bg-slate-200'"></div>

        <!-- 格式转换胶囊组 (JSON / XML / YAML / 结构体) -->
        <div
          class="inline-flex items-center p-0.5 rounded-lg border text-[11px]"
          :class="
            store.isDark
              ? 'bg-slate-900/90 border-slate-800'
              : 'bg-slate-100/90 border-slate-200/80'
          "
        >
          <button
            type="button"
            class="px-2 py-0.5 rounded-md font-medium cursor-pointer transition-colors"
            :class="
              outputMode === 'json' || outputMode === 'minified' || outputMode === 'escaped'
                ? store.isDark
                  ? 'bg-indigo-500/20 text-indigo-300 font-semibold'
                  : 'bg-white text-indigo-600 font-semibold shadow-2xs'
                : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
            "
            @click="outputMode = 'json'"
          >
            JSON
          </button>
          <button
            type="button"
            class="px-2 py-0.5 rounded-md font-medium cursor-pointer transition-colors"
            :class="
              outputMode === 'xml'
                ? store.isDark
                  ? 'bg-indigo-500/20 text-indigo-300 font-semibold'
                  : 'bg-white text-indigo-600 font-semibold shadow-2xs'
                : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
            "
            @click="handleXmlConvert"
          >
            XML
          </button>
          <button
            type="button"
            class="px-2 py-0.5 rounded-md font-medium cursor-pointer transition-colors"
            :class="
              outputMode === 'yaml'
                ? store.isDark
                  ? 'bg-indigo-500/20 text-indigo-300 font-semibold'
                  : 'bg-white text-indigo-600 font-semibold shadow-2xs'
                : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
            "
            @click="handleYamlConvert"
          >
            YAML
          </button>
          <button
            type="button"
            class="px-2 py-0.5 rounded-md font-medium cursor-pointer transition-colors"
            :class="
              outputMode === 'struct'
                ? store.isDark
                  ? 'bg-indigo-500/20 text-indigo-300 font-semibold'
                  : 'bg-white text-indigo-600 font-semibold shadow-2xs'
                : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
            "
            @click="outputMode = 'struct'"
          >
            结构体
          </button>
        </div>
      </div>

      <!-- Right Validation Status Badge (Clickable to Validate) -->
      <div class="flex items-center gap-2 shrink-0">
        <button
          v-if="parsedResult.ok"
          type="button"
          class="flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[11px] font-mono border cursor-pointer transition-opacity hover:opacity-85"
          :class="
            store.isDark
              ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
              : 'bg-indigo-50 text-indigo-600 border-indigo-200/80'
          "
          @click="handleValidateAction"
        >
          <IconRenderer name="Check" :size="12" />
          <span>JSON 语法有效</span>
        </button>
        <button
          v-else
          type="button"
          class="flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[11px] font-mono bg-rose-500/15 text-rose-400 border border-rose-500/30 max-w-[220px] truncate cursor-pointer"
          :title="parsedResult.error || ''"
          @click="handleValidateAction"
        >
          <span>{{ parsedResult.error }}</span>
        </button>
      </div>
    </div>

    <!-- ======================================================================= -->
    <!-- Split Workspace: Left Raw JSON / Right Inspector & Converter            -->
    <!-- ======================================================================= -->
    <div
      class="flex-1 grid grid-cols-2 min-h-0 divide-x overflow-hidden"
      :class="store.isDark ? 'divide-slate-800' : 'divide-slate-200'"
    >
      <!-- ===================================================================== -->
      <!-- Left Pane: 源 JSON 编辑器                                              -->
      <!-- ===================================================================== -->
      <div class="flex flex-col min-h-0 overflow-hidden">
        <!-- Left Subheader -->
        <div
          class="flex items-center justify-between px-3 py-1.5 text-[11px] border-b shrink-0"
          :class="
            store.isDark
              ? 'bg-obsidian-950/30 border-slate-800/70 text-slate-400'
              : 'bg-slate-50 border-slate-200 text-slate-500'
          "
        >
          <div class="flex items-center gap-2">
            <span class="font-mono">源 JSON 编辑器 ({{ inputStats }})</span>
            <div
              class="inline-flex items-center rounded border p-0.5 text-[10px]"
              :class="
                store.isDark
                  ? 'border-slate-800 bg-slate-900'
                  : 'border-slate-200/90 bg-slate-200/50'
              "
            >
              <button
                type="button"
                class="px-1.5 py-0.2 rounded cursor-pointer transition-colors"
                :class="
                  leftViewMode === 'edit'
                    ? store.isDark
                      ? 'bg-indigo-500/25 text-indigo-300 font-medium'
                      : 'bg-white text-indigo-600 font-medium shadow-2xs'
                    : 'text-slate-400 hover:text-slate-600'
                "
                @click="leftViewMode = 'edit'"
              >
                编辑
              </button>
              <button
                type="button"
                class="px-1.5 py-0.2 rounded cursor-pointer transition-colors"
                :class="
                  leftViewMode === 'fold'
                    ? store.isDark
                      ? 'bg-indigo-500/25 text-indigo-300 font-medium'
                      : 'bg-white text-indigo-600 font-medium shadow-2xs'
                    : 'text-slate-400 hover:text-slate-600'
                "
                @click="leftViewMode = 'fold'"
              >
                折叠
              </button>
            </div>
          </div>

          <button
            type="button"
            class="flex items-center gap-1 hover:text-indigo-500 cursor-pointer transition-colors"
            @click="copyText(rawInput, '已复制源 JSON 原文')"
          >
            <IconRenderer name="Copy" :size="12" />
            <span>复制原文</span>
          </button>
        </div>

        <!-- Left Editable Textarea with Line Numbers -->
        <div v-if="leftViewMode === 'edit'" class="relative flex-1 flex min-h-0 overflow-hidden">
          <div
            class="w-8 py-3 select-none text-right pr-2 font-mono text-[11px] leading-[1.65] border-r overflow-hidden shrink-0"
            :class="
              store.isDark
                ? 'bg-obsidian-950/25 border-slate-800/50 text-slate-600'
                : 'bg-slate-50/60 border-slate-100 text-slate-400'
            "
          >
            <div v-for="n in inputLineNumbers" :key="n">{{ n }}</div>
          </div>

          <textarea
            v-model="rawInput"
            spellcheck="false"
            aria-label="JSON 源码编辑框"
            placeholder="在此粘贴或输入 JSON..."
            class="flex-1 w-full p-3 bg-transparent font-mono text-[12px] leading-[1.65] resize-none outline-none overflow-y-auto select-text"
            :class="store.isDark ? 'text-slate-100' : 'text-slate-800'"
          ></textarea>
        </div>

        <!-- Left Collapsible View -->
        <div
          v-else
          class="flex-1 overflow-y-auto py-2.5 px-2 font-mono text-[12px] leading-[1.65] select-text"
        >
          <div
            v-for="line in leftJsonLines"
            :key="line.path + '-' + line.lineNo"
            class="flex items-baseline hover:bg-indigo-500/5 rounded px-1"
          >
            <span class="w-7 text-right pr-2 text-[11px] text-slate-400 select-none shrink-0">
              {{ line.lineNo }}
            </span>
            <button
              v-if="line.collapsible"
              type="button"
              class="w-4 text-center text-slate-400 hover:text-indigo-500 cursor-pointer select-none shrink-0"
              @click="toggleFoldPath('left', line.path)"
            >
              {{ line.collapsed ? '▸' : '▾' }}
            </button>
            <span v-else class="w-4 shrink-0"></span>

            <div class="whitespace-pre break-all">
              <span>{{ indentSpacesString.repeat(line.indentLevel) }}</span>
              <span
                v-if="line.keyName !== undefined"
                :class="store.isDark ? 'text-indigo-300' : 'text-indigo-700'"
              >
                "{{ line.keyName }}"
              </span>
              <span v-if="line.keyName !== undefined" class="text-slate-400">: </span>

              <template v-if="line.collapsed">
                <span class="text-slate-500">{{ line.openBracket }}</span>
                <button
                  type="button"
                  class="mx-1 px-1.5 py-0.2 text-[10px] rounded bg-indigo-500/10 text-indigo-500 cursor-pointer"
                  @click="toggleFoldPath('left', line.path)"
                >
                  ... {{ line.collapsedSummary }}
                </button>
                <span class="text-slate-500">{{ line.closeBracket }}</span>
              </template>
              <template v-else>
                <span v-if="line.openBracket" class="text-slate-500">{{ line.openBracket }}</span>
                <span
                  v-if="line.valueText !== undefined"
                  :class="[
                    line.valueType === 'string'
                      ? store.isDark
                        ? 'text-emerald-300'
                        : 'text-emerald-700'
                      : line.valueType === 'number'
                        ? store.isDark
                          ? 'text-amber-300'
                          : 'text-blue-700'
                        : line.valueType === 'boolean' || line.valueType === 'null'
                          ? store.isDark
                            ? 'text-violet-300 font-semibold'
                            : 'text-violet-700 font-semibold'
                          : ''
                  ]"
                >
                  {{ line.valueText }}
                </span>
                <span v-if="line.closeBracket" class="text-slate-500">{{ line.closeBracket }}</span>
              </template>
              <span v-if="line.trailingComma" class="text-slate-400">,</span>
            </div>
          </div>
        </div>

        <!-- Left Footer: 全部折叠 / 全部展开 -->
        <div
          class="flex items-center justify-between px-3 py-1 border-t text-[11px] shrink-0"
          :class="
            store.isDark
              ? 'bg-obsidian-950/30 border-slate-800/70 text-slate-400'
              : 'bg-slate-50/80 border-slate-200/80 text-slate-500'
          "
        >
          <div class="flex items-center gap-3">
            <button
              type="button"
              class="hover:text-indigo-500 cursor-pointer transition-colors"
              @click="foldAll('left')"
            >
              全部折叠
            </button>
            <button
              type="button"
              class="hover:text-indigo-500 cursor-pointer transition-colors"
              @click="unfoldAll('left')"
            >
              全部展开
            </button>
          </div>
          <button
            v-if="leftViewMode === 'fold'"
            type="button"
            class="text-indigo-500 hover:underline cursor-pointer"
            @click="leftViewMode = 'edit'"
          >
            返回编辑
          </button>
        </div>
      </div>

      <!-- ===================================================================== -->
      <!-- Right Pane: 节点预览与转换输出                                         -->
      <!-- ===================================================================== -->
      <div class="flex flex-col min-h-0 overflow-hidden">
        <!-- Right Subheader -->
        <div
          class="flex items-center justify-between gap-2 px-3 py-1.5 border-b text-[11px] shrink-0"
          :class="
            store.isDark
              ? 'bg-obsidian-950/30 border-slate-800/70 text-slate-400'
              : 'bg-slate-50 border-slate-200 text-slate-500'
          "
        >
          <div class="flex items-center gap-2 shrink-0">
            <span
              class="px-1.5 py-0.2 rounded text-[10px] font-mono font-semibold border"
              :class="
                store.isDark
                  ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
                  : 'bg-indigo-50 text-indigo-600 border-indigo-200/80'
              "
            >
              {{ outputModeLabel }}
            </span>

            <div v-if="outputMode === 'struct'" class="flex items-center gap-1 font-mono">
              <button
                v-for="lang in (['ts', 'rust', 'go'] as const)"
                :key="lang"
                type="button"
                class="px-1.5 py-0.2 rounded uppercase text-[10px] cursor-pointer"
                :class="
                  structLang === lang
                    ? 'bg-indigo-600 text-white'
                    : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
                "
                @click="structLang = lang"
              >
                {{ lang }}
              </button>
            </div>
            <span v-else class="text-[11px] truncate">输出结果预览</span>
          </div>

          <!-- Right Controls: 搜索 + 缩进 + 复制 + 下载 + 保存 -->
          <div class="flex items-center gap-2 text-[11px] shrink-0">
            <button
              type="button"
              title="搜索或过滤 JSON 内容 (Ctrl+F / ⌘F)"
              class="flex items-center gap-1 px-1.5 py-0.5 rounded border cursor-pointer transition-colors"
              :class="
                showSearchBar
                  ? store.isDark
                    ? 'bg-indigo-500/20 border-indigo-500/50 text-indigo-300 font-medium'
                    : 'bg-indigo-50 border-indigo-300 text-indigo-600 font-medium'
                  : store.isDark
                    ? 'bg-slate-900 border-slate-700/80 text-slate-300 hover:border-indigo-500/40'
                    : 'bg-white border-slate-200 text-slate-600 hover:border-indigo-300'
              "
              @click="showSearchBar ? closeSearchBar() : openSearchBar()"
            >
              <IconRenderer name="Search" :size="11" />
              <span>搜索</span>
            </button>

            <div class="flex items-center gap-1">
              <span>缩进</span>
              <select
                v-model="indentOption"
                aria-label="调整缩进空格数"
                class="h-[20px] px-1 rounded border text-[10.5px] font-mono outline-none cursor-pointer"
                :class="
                  store.isDark
                    ? 'bg-slate-900 border-slate-700 text-slate-200'
                    : 'bg-white border-slate-200 text-slate-700'
                "
              >
                <option :value="2">2 空格</option>
                <option :value="4">4 空格</option>
                <option value="tab">Tab</option>
              </select>
            </div>

            <button
              type="button"
              class="flex items-center gap-1 hover:text-indigo-500 cursor-pointer transition-colors"
              @click="copyText(formattedOutputText, `已复制 ${outputModeLabel} 结果`)"
            >
              <IconRenderer name="Copy" :size="11" />
              <span>复制</span>
            </button>

            <button
              type="button"
              class="flex items-center gap-1 hover:text-indigo-500 cursor-pointer transition-colors"
              @click="downloadOutputFile"
            >
              <IconRenderer name="Download" :size="11" />
              <span>下载</span>
            </button>

            <button
              type="button"
              class="flex items-center gap-1 hover:text-indigo-500 cursor-pointer transition-colors"
              @click="saveToHistory"
            >
              <IconRenderer name="Bookmark" :size="11" />
              <span>保存</span>
            </button>

            <button
              v-if="historyList.length > 0"
              type="button"
              class="text-indigo-500 hover:underline cursor-pointer font-mono"
              @click="showHistoryModal = !showHistoryModal"
            >
              历史({{ historyList.length }})
            </button>
          </div>
        </div>

        <!-- =================================================================== -->
        <!-- Inline Content Search & Filter Bar (Ctrl+F / Command+F)             -->
        <!-- =================================================================== -->
        <div
          v-if="showSearchBar"
          class="flex items-center justify-between gap-2 px-3 py-1.5 border-b text-xs shrink-0"
          :class="
            store.isDark
              ? 'bg-[#131B2E] border-indigo-500/30 text-slate-200'
              : 'bg-indigo-50/60 border-indigo-200/80 text-slate-800'
          "
        >
          <div class="flex items-center gap-1.5 flex-1 min-w-0">
            <IconRenderer name="Search" :size="13" class-name="text-indigo-400 shrink-0" />
            <input
              ref="searchInputRef"
              v-model="searchInputText"
              type="text"
              autocomplete="off"
              spellcheck="false"
              placeholder="查找 Key 或 Value (Enter 下一个, Shift+Enter 上一个)..."
              aria-label="在输出结果中搜索关键词"
              class="flex-1 min-w-0 h-6 px-2 rounded border text-[12px] font-mono outline-none transition-colors"
              :class="
                store.isDark
                  ? 'bg-slate-900/90 border-slate-700 text-slate-100 focus:border-indigo-500 placeholder:text-slate-500'
                  : 'bg-white border-slate-300/90 text-slate-800 focus:border-indigo-500 placeholder:text-slate-400'
              "
              @keydown="handleSearchInputKeydown"
            />
          </div>

          <div class="flex items-center gap-1 shrink-0">
            <!-- Match Counter -->
            <span
              class="px-1.5 text-[11px] font-mono tabular-nums"
              :class="
                searchKeyword.trim() && activeMatchedLineNos.length === 0
                  ? 'text-rose-400 font-semibold'
                  : 'text-slate-400'
              "
            >
              <template v-if="!searchKeyword.trim()">0 / 0</template>
              <template v-else-if="activeMatchedLineNos.length === 0">无结果</template>
              <template v-else>
                {{ currentMatchIdx + 1 }} / {{ activeMatchedLineNos.length }}
              </template>
            </span>

            <!-- Prev / Next Match Buttons -->
            <button
              type="button"
              title="上一个匹配项 (Shift+Enter)"
              :disabled="activeMatchedLineNos.length === 0"
              class="w-6 h-6 flex items-center justify-center rounded border text-[11px] cursor-pointer disabled:opacity-35 transition-colors"
              :class="
                store.isDark
                  ? 'bg-slate-800 border-slate-700 hover:bg-slate-700'
                  : 'bg-white border-slate-200 hover:bg-slate-100'
              "
              @click="gotoPrevMatch"
            >
              ↑
            </button>
            <button
              type="button"
              title="下一个匹配项 (Enter)"
              :disabled="activeMatchedLineNos.length === 0"
              class="w-6 h-6 flex items-center justify-center rounded border text-[11px] cursor-pointer disabled:opacity-35 transition-colors"
              :class="
                store.isDark
                  ? 'bg-slate-800 border-slate-700 hover:bg-slate-700'
                  : 'bg-white border-slate-200 hover:bg-slate-100'
              "
              @click="gotoNextMatch"
            >
              ↓
            </button>

            <!-- Case Sensitive Toggle (Aa) -->
            <button
              type="button"
              title="区分大小写 (Aa)"
              class="px-1.5 h-6 rounded border font-mono text-[11px] cursor-pointer transition-colors"
              :class="
                caseSensitive
                  ? 'bg-indigo-600 border-indigo-600 text-white font-semibold'
                  : store.isDark
                    ? 'bg-slate-800 border-slate-700 text-slate-400 hover:text-slate-200'
                    : 'bg-white border-slate-200 text-slate-500 hover:text-slate-800'
              "
              @click="caseSensitive = !caseSensitive"
            >
              Aa
            </button>

            <!-- Filter Matched Lines Only Toggle (过滤仅看匹配行) -->
            <button
              type="button"
              title="仅显示包含搜索词的匹配行（过滤非匹配内容）"
              class="px-2 h-6 rounded border text-[11px] font-medium cursor-pointer transition-colors"
              :class="
                filterMatchedOnly
                  ? 'bg-indigo-600 border-indigo-600 text-white font-semibold'
                  : store.isDark
                    ? 'bg-slate-800 border-slate-700 text-slate-300 hover:text-indigo-300'
                    : 'bg-white border-slate-200 text-slate-600 hover:text-indigo-600'
              "
              @click="filterMatchedOnly = !filterMatchedOnly"
            >
              仅匹配行
            </button>

            <!-- Close Search Bar -->
            <button
              type="button"
              title="关闭搜索栏 (Esc)"
              class="w-6 h-6 flex items-center justify-center rounded text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
              @click="closeSearchBar"
            >
              <IconRenderer name="X" :size="13" />
            </button>
          </div>
        </div>

        <!-- Right Body: Collapsible JSON Tree View (Virtualized + On-Demand Highlight + v-memo) -->
        <div
          v-if="outputMode === 'json' && parsedResult.ok && rightJsonLines.length > 0"
          ref="rightScrollContainerRef"
          class="flex-1 overflow-y-auto py-2.5 px-2 font-mono text-[12px] leading-[20px] select-text"
          @scroll.passive="handleRightTreeScroll"
        >
          <div
            v-if="displayedRightJsonLines.length === 0 && searchKeyword.trim()"
            class="py-12 text-center text-xs text-slate-400"
          >
            未找到包含 "{{ searchKeyword }}" 的匹配内容
          </div>

          <div
            v-if="virtualRightJsonWindow.topPad > 0"
            :style="{ height: virtualRightJsonWindow.topPad + 'px' }"
          />

          <div
            v-for="line in virtualRightJsonWindow.items"
            :key="line.path + '-' + line.lineNo"
            v-memo="[
              line.path,
              line.collapsed,
              line.indentLevel,
              indentOption,
              store.isDark,
              filterMatchedOnly,
              matchedLineNoSet.has(line.lineNo),
              currentMatchedLineNo === line.lineNo,
              matchedLineNoSet.has(line.lineNo) ? searchKeyword : '',
              caseSensitive
            ]"
            :data-right-line="line.lineNo"
            class="flex items-baseline rounded px-1 min-h-[20px]"
            :class="[
              currentMatchedLineNo === line.lineNo
                ? store.isDark
                  ? 'bg-amber-500/20 ring-1 ring-amber-400/60'
                  : 'bg-amber-100/90 ring-1 ring-amber-400'
                : matchedLineNoSet.has(line.lineNo)
                  ? store.isDark
                    ? 'bg-indigo-500/10'
                    : 'bg-amber-50/70'
                  : 'hover:bg-indigo-500/5'
            ]"
          >
            <span class="w-8 text-right pr-2 text-[11px] text-slate-400 select-none shrink-0">
              {{ line.lineNo }}
            </span>

            <button
              v-if="line.collapsible && !filterMatchedOnly"
              type="button"
              class="w-4 text-center text-slate-400 hover:text-indigo-500 cursor-pointer select-none shrink-0"
              @click="toggleFoldPath('right', line.path)"
            >
              {{ line.collapsed ? '▸' : '▾' }}
            </button>
            <span v-else class="w-4 shrink-0"></span>

            <div class="whitespace-pre break-all">
              <span>{{
                indentSpacesString.repeat(filterMatchedOnly ? 0 : line.indentLevel)
              }}</span>
              <span
                v-if="line.keyName !== undefined"
                :class="store.isDark ? 'text-indigo-300' : 'text-indigo-700'"
              >
                <template v-if="matchedLineNoSet.has(line.lineNo)">"<template
                    v-for="(seg, sIdx) in splitHighlightParts(line.keyName)"
                    :key="sIdx"
                  ><mark
                      v-if="seg.highlight"
                      class="rounded px-0.5"
                      :class="
                        currentMatchedLineNo === line.lineNo
                          ? 'bg-amber-400 text-slate-950 font-bold'
                          : 'bg-amber-300/85 dark:bg-amber-500/45 text-slate-950 dark:text-amber-100'
                      "
                      >{{ seg.text }}</mark
                    ><template v-else>{{ seg.text }}</template></template
                  >"</template>
                <template v-else>"{{ line.keyName }}"</template>
              </span>
              <span v-if="line.keyName !== undefined" class="text-slate-400">: </span>

              <template v-if="line.collapsed">
                <span class="text-slate-500">{{ line.openBracket }}</span>
                <button
                  type="button"
                  class="mx-1 px-1.5 py-0.2 text-[10px] rounded bg-indigo-500/10 text-indigo-500 cursor-pointer"
                  @click="toggleFoldPath('right', line.path)"
                >
                  ... {{ line.collapsedSummary }}
                </button>
                <span class="text-slate-500">{{ line.closeBracket }}</span>
              </template>
              <template v-else>
                <span v-if="line.openBracket" class="text-slate-500">{{ line.openBracket }}</span>
                <span
                  v-if="line.valueText !== undefined"
                  :class="[
                    line.valueType === 'string'
                      ? store.isDark
                        ? 'text-emerald-300'
                        : 'text-emerald-700'
                      : line.valueType === 'number'
                        ? store.isDark
                          ? 'text-amber-300'
                          : 'text-blue-700'
                        : line.valueType === 'boolean' || line.valueType === 'null'
                          ? store.isDark
                            ? 'text-violet-300 font-semibold'
                            : 'text-violet-700 font-semibold'
                          : ''
                  ]"
                >
                  <template v-if="matchedLineNoSet.has(line.lineNo)">
                    <template
                      v-for="(seg, vIdx) in splitHighlightParts(line.valueText)"
                      :key="vIdx"
                    ><mark
                        v-if="seg.highlight"
                        class="rounded px-0.5"
                        :class="
                          currentMatchedLineNo === line.lineNo
                            ? 'bg-amber-400 text-slate-950 font-bold'
                            : 'bg-amber-300/85 dark:bg-amber-500/45 text-slate-950 dark:text-amber-100'
                        "
                        >{{ seg.text }}</mark
                      ><template v-else>{{ seg.text }}</template></template
                    >
                  </template>
                  <template v-else>{{ line.valueText }}</template>
                </span>
                <span v-if="line.closeBracket" class="text-slate-500">{{ line.closeBracket }}</span>
              </template>
              <span v-if="line.trailingComma" class="text-slate-400">,</span>
            </div>
          </div>

          <div
            v-if="virtualRightJsonWindow.bottomPad > 0"
            :style="{ height: virtualRightJsonWindow.bottomPad + 'px' }"
          />
        </div>

        <!-- XML / YAML / Minified / Escaped / Struct Output with Line Numbers (Virtualized + On-Demand Highlight) -->
        <div
          v-else
          ref="rightRawScrollContainerRef"
          class="flex-1 overflow-y-auto py-2.5 px-2 font-mono text-[12px] leading-[20px] select-text"
          @scroll.passive="handleRightRawScroll"
        >
          <div
            v-if="virtualRawWindow.topPad > 0"
            :style="{ height: virtualRawWindow.topPad + 'px' }"
          />

          <div
            v-for="item in virtualRawWindow.items"
            :key="item.lineNo"
            v-memo="[
              item.lineNo,
              item.text,
              store.isDark,
              matchedLineNoSet.has(item.lineNo),
              currentMatchedLineNo === item.lineNo,
              matchedLineNoSet.has(item.lineNo) ? searchKeyword : '',
              caseSensitive
            ]"
            :data-right-line="item.lineNo"
            class="flex items-baseline rounded px-1 min-h-[20px]"
            :class="[
              currentMatchedLineNo === item.lineNo
                ? store.isDark
                  ? 'bg-amber-500/20 ring-1 ring-amber-400/60'
                  : 'bg-amber-100/90 ring-1 ring-amber-400'
                : matchedLineNoSet.has(item.lineNo)
                  ? store.isDark
                    ? 'bg-indigo-500/10'
                    : 'bg-amber-50/70'
                  : 'hover:bg-indigo-500/5'
            ]"
          >
            <span class="w-8 text-right pr-3 text-[11px] text-slate-400 select-none shrink-0">
              {{ item.lineNo }}
            </span>
            <pre
              class="whitespace-pre-wrap break-all m-0 font-mono"
              :class="
                !parsedResult.ok
                  ? 'text-rose-500'
                  : store.isDark
                    ? 'text-indigo-200'
                    : 'text-slate-800'
              "
            ><template v-if="matchedLineNoSet.has(item.lineNo)"><template
              v-for="(seg, rIdx) in splitHighlightParts(item.text)"
              :key="rIdx"
            ><mark
              v-if="seg.highlight"
              class="rounded px-0.5"
              :class="
                currentMatchedLineNo === item.lineNo
                  ? 'bg-amber-400 text-slate-950 font-bold'
                  : 'bg-amber-300/85 dark:bg-amber-500/45 text-slate-950 dark:text-amber-100'
              "
              >{{ seg.text }}</mark
            ><template v-else>{{ seg.text }}</template></template></template><template v-else>{{ item.text }}</template></pre>
          </div>

          <div
            v-if="virtualRawWindow.bottomPad > 0"
            :style="{ height: virtualRawWindow.bottomPad + 'px' }"
          />
        </div>

        <!-- Right Footer: 全部折叠 / 全部展开 -->
        <div
          class="flex items-center justify-between px-3 py-1 border-t text-[11px] shrink-0"
          :class="
            store.isDark
              ? 'bg-obsidian-950/30 border-slate-800/70 text-slate-400'
              : 'bg-slate-50/80 border-slate-200/80 text-slate-500'
          "
        >
          <div class="flex items-center gap-3">
            <button
              type="button"
              class="hover:text-indigo-500 cursor-pointer transition-colors"
              @click="foldAll('right')"
            >
              全部折叠
            </button>
            <button
              type="button"
              class="hover:text-indigo-500 cursor-pointer transition-colors"
              @click="unfoldAll('right')"
            >
              全部展开
            </button>
          </div>

          <span class="font-mono text-[10.5px]">{{ outputStats }}</span>
        </div>
      </div>
    </div>

    <!-- Saved Snapshots History Drawer -->
    <div
      v-if="showHistoryModal"
      class="border-t px-4 py-2.5 max-h-[150px] overflow-y-auto space-y-1.5 text-xs shrink-0"
      :class="store.isDark ? 'bg-[#0A0E17] border-slate-800' : 'bg-white border-slate-200'"
    >
      <div class="flex items-center justify-between text-[11px] text-slate-400">
        <span class="font-semibold">已保存的 JSON 快照历史 ({{ historyList.length }})</span>
        <button
          type="button"
          class="hover:text-indigo-500 cursor-pointer"
          @click="showHistoryModal = false"
        >
          关闭
        </button>
      </div>
      <div class="grid grid-cols-2 gap-2">
        <div
          v-for="item in historyList"
          :key="item.id"
          class="flex items-center justify-between px-2.5 py-1.5 rounded-lg border cursor-pointer"
          :class="
            store.isDark
              ? 'bg-slate-900 border-slate-800 hover:border-indigo-500/50'
              : 'bg-slate-50 border-slate-200/80 hover:border-indigo-400'
          "
          @click="
            () => {
              rawInput = item.content
              showHistoryModal = false
            }
          "
        >
          <span class="font-mono text-[11px] truncate pr-2">{{ item.title }}</span>
          <div class="flex items-center gap-2 shrink-0 text-[10px] text-slate-400">
            <span>{{ item.savedAt }}</span>
            <button
              type="button"
              class="hover:text-rose-500 cursor-pointer"
              @click.stop="deleteHistoryEntry(item.id)"
            >
              删除
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
