<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import IconRenderer from '@/components/IconRenderer.vue'

interface InlineSegment {
  text: string
  changed: boolean
}

interface PairedDiffRow {
  rowKey: string
  kind: 'same' | 'modified' | 'removed' | 'added'
  leftLineNo: number | null
  rightLineNo: number | null
  leftText: string
  rightText: string
  leftSegments: InlineSegment[]
  rightSegments: InlineSegment[]
}

interface SameBlock {
  blockType: 'same'
  id: string
  rows: PairedDiffRow[]
}

interface ChangeHunk {
  blockType: 'hunk'
  id: string
  hunkIndex: number // 1-based index (1..totalHunks)
  leftStartIdx: number // 0-based line index in leftLinesArray
  leftCount: number
  leftRawLines: string[]
  rightStartIdx: number // 0-based line index in rightLinesArray
  rightCount: number
  rightRawLines: string[]
  rows: PairedDiffRow[]
}

type DiffBlock = SameBlock | ChangeHunk

interface UndoSnapshot {
  leftText: string
  rightText: string
  description: string
}

const DRAFT_STORAGE_KEY = 'omnibox:live-draft:diff-viewer'
const CONTEXT_LINES = 3
const MIN_FOLDABLE_SAME_LINES = 8

const DEFAULT_LEFT_TEXT = `apiVersion: apps/v1
kind: Deployment
metadata:
  annotations:
    deployment.kubernetes.io/revision: "320"
    kubernetes.io/change-cause: kubectl set image Deployment/data-center data-center=registry-intl.ap-southeast-1.aliyuncs.com/relax_many/data-center:c8a6f10-10210947-ocv
    --namespace=api-center --record=true
    meta.helm.sh/release-name: data-center
    meta.helm.sh/release-namespace: api-center
  creationTimestamp: "2025-07-01T08:39:16Z"
  generation: 326
  labels:
    app: data-center
    app.kubernetes.io/managed-by: Helm
  name: data-center
  namespace: api-center`

const DEFAULT_RIGHT_TEXT = `apiVersion: apps/v1
kind: StatefulSet
metadata:
  annotations:
    kubernetes.io/change-cause: kubectl set image StatefulSet/business-sts business-sts=registry-intl.ap-southeast-1.aliyuncs.com/relax_many/business-sts:c0f4eb1-10211800-dev-rc
    --namespace=api-center --record=true
    meta.helm.sh/release-name: business-sts
    meta.helm.sh/release-namespace: api-center
  creationTimestamp: "2026-10-21T08:01:28Z"
  generation: 8
  labels:
    app: business-sts
    app.kubernetes.io/instance: business-sts
    app.kubernetes.io/managed-by: Helm
  name: business-sts
  namespace: api-center`

const store = useLauncherStore()

const leftText = ref(DEFAULT_LEFT_TEXT)
const rightText = ref(DEFAULT_RIGHT_TEXT)
const viewMode = ref<'split' | 'edit'>('split')
const wordWrap = ref(true)
const collapseUnchanged = ref(true)
const ignoreWhitespace = ref(false)
const onlyShowChanges = ref(false)

const activeHunkIndex = ref<number | null>(null)
const expandedSameBlockIds = ref<Set<string>>(new Set())
const undoStack = ref<UndoSnapshot[]>([])
const diffScrollContainerRef = ref<HTMLElement | null>(null)

onMounted(() => {
  const winObj = window as unknown as {
    __omniboxPluginInterceptEscape?: () => boolean
  }
  winObj.__omniboxPluginInterceptEscape = () => {
    if (activeHunkIndex.value !== null) {
      activeHunkIndex.value = null
      return true
    }
    return false
  }
  try {
    const rawDraft = localStorage.getItem(DRAFT_STORAGE_KEY)
    if (rawDraft) {
      const parsed = JSON.parse(rawDraft) as {
        leftText?: string
        rightText?: string
        viewMode?: 'split' | 'edit'
        wordWrap?: boolean
        collapseUnchanged?: boolean
        ignoreWhitespace?: boolean
        onlyShowChanges?: boolean
      }
      if (typeof parsed.leftText === 'string') leftText.value = parsed.leftText
      if (typeof parsed.rightText === 'string') rightText.value = parsed.rightText
      if (parsed.viewMode === 'split' || parsed.viewMode === 'edit') {
        viewMode.value = parsed.viewMode
      }
      if (typeof parsed.wordWrap === 'boolean') wordWrap.value = parsed.wordWrap
      if (typeof parsed.collapseUnchanged === 'boolean') {
        collapseUnchanged.value = parsed.collapseUnchanged
      }
      if (typeof parsed.ignoreWhitespace === 'boolean') {
        ignoreWhitespace.value = parsed.ignoreWhitespace
      }
      if (typeof parsed.onlyShowChanges === 'boolean') {
        onlyShowChanges.value = parsed.onlyShowChanges
      }
    }
  } catch {
    // ignore corrupted draft
  }
  window.addEventListener('keydown', handlePluginKeydown, true)
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

watch(
  [leftText, rightText, viewMode, wordWrap, collapseUnchanged, ignoreWhitespace, onlyShowChanges],
  () => {
    try {
      localStorage.setItem(
        DRAFT_STORAGE_KEY,
        JSON.stringify({
          leftText: leftText.value,
          rightText: rightText.value,
          viewMode: viewMode.value,
          wordWrap: wordWrap.value,
          collapseUnchanged: collapseUnchanged.value,
          ignoreWhitespace: ignoreWhitespace.value,
          onlyShowChanges: onlyShowChanges.value
        })
      )
    } catch {
      // ignore storage quota errors
    }
  }
)

function normalizeLine(line: string): string {
  return ignoreWhitespace.value ? line.trim().replace(/\s+/g, ' ') : line
}

/**
 * Compute character/token-level inline diff segments between a paired left & right line.
 * Highlights the exact words/tokens that changed (just like Diffchecker).
 */
function computeInlineDiff(
  leftStr: string,
  rightStr: string
): { leftSegments: InlineSegment[]; rightSegments: InlineSegment[] } {
  if (leftStr === rightStr) {
    return {
      leftSegments: [{ text: leftStr, changed: false }],
      rightSegments: [{ text: rightStr, changed: false }]
    }
  }

  // Tokenize by word / punctuation / whitespace boundaries
  const tokenize = (s: string): string[] =>
    s.match(/(\s+|[a-zA-Z0-9_]+|[\u4e00-\u9fa5]|[^a-zA-Z0-9_\s\u4e00-\u9fa5])/g) || []

  const aTokens = tokenize(leftStr)
  const bTokens = tokenize(rightStr)

  // Fast prefix/suffix fallback if a single line has too many tokens (> 160)
  if (aTokens.length * bTokens.length > 25600) {
    let prefixLen = 0
    const minLen = Math.min(leftStr.length, rightStr.length)
    while (prefixLen < minLen && leftStr[prefixLen] === rightStr[prefixLen]) {
      prefixLen++
    }
    let suffixLen = 0
    while (
      suffixLen < minLen - prefixLen &&
      leftStr[leftStr.length - 1 - suffixLen] === rightStr[rightStr.length - 1 - suffixLen]
    ) {
      suffixLen++
    }

    const buildFallback = (str: string): InlineSegment[] => {
      const segs: InlineSegment[] = []
      const pre = str.slice(0, prefixLen)
      const mid = str.slice(prefixLen, str.length - suffixLen)
      const suf = suffixLen > 0 ? str.slice(str.length - suffixLen) : ''
      if (pre) segs.push({ text: pre, changed: false })
      if (mid) segs.push({ text: mid, changed: true })
      if (suf) segs.push({ text: suf, changed: false })
      return segs.length > 0 ? segs : [{ text: str, changed: false }]
    }

    return {
      leftSegments: buildFallback(leftStr),
      rightSegments: buildFallback(rightStr)
    }
  }

  // Token-level LCS
  const n = aTokens.length
  const m = bTokens.length
  const dp: number[][] = Array.from({ length: n + 1 }, () => Array(m + 1).fill(0))
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      if (aTokens[i] === bTokens[j]) {
        dp[i]![j] = 1 + dp[i + 1]![j + 1]!
      } else {
        dp[i]![j] = Math.max(dp[i + 1]![j]!, dp[i]![j + 1]!)
      }
    }
  }

  const pushMerged = (arr: InlineSegment[], text: string, changed: boolean) => {
    if (!text) return
    const last = arr[arr.length - 1]
    if (last && last.changed === changed) {
      last.text += text
    } else {
      arr.push({ text, changed })
    }
  }

  const leftSegments: InlineSegment[] = []
  const rightSegments: InlineSegment[] = []
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (aTokens[i] === bTokens[j]) {
      pushMerged(leftSegments, aTokens[i]!, false)
      pushMerged(rightSegments, bTokens[j]!, false)
      i++
      j++
    } else if (dp[i + 1]![j]! >= dp[i]![j + 1]!) {
      pushMerged(leftSegments, aTokens[i]!, true)
      i++
    } else {
      pushMerged(rightSegments, bTokens[j]!, true)
      j++
    }
  }
  while (i < n) {
    pushMerged(leftSegments, aTokens[i++]!, true)
  }
  while (j < m) {
    pushMerged(rightSegments, bTokens[j++]!, true)
  }

  return {
    leftSegments: leftSegments.length > 0 ? leftSegments : [{ text: leftStr, changed: false }],
    rightSegments: rightSegments.length > 0 ? rightSegments : [{ text: rightStr, changed: false }]
  }
}

const leftLinesArray = computed(() => leftText.value.split('\n'))
const rightLinesArray = computed(() => rightText.value.split('\n'))

/**
 * Full Diff Pipeline:
 * 1. Line-level LCS
 * 2. Group contiguous changes into aligned ChangeHunks & SameBlocks
 * 3. Pair removed+added lines side-by-side and compute inline token highlights
 */
const diffBlocks = computed<DiffBlock[]>(() => {
  const aLines = leftLinesArray.value
  const bLines = rightLinesArray.value
  const n = aLines.length
  const m = bLines.length

  const dp: number[][] = Array.from({ length: n + 1 }, () => Array(m + 1).fill(0))
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      if (normalizeLine(aLines[i]!) === normalizeLine(bLines[j]!)) {
        dp[i]![j] = 1 + dp[i + 1]![j + 1]!
      } else {
        dp[i]![j] = Math.max(dp[i + 1]![j]!, dp[i]![j + 1]!)
      }
    }
  }

  interface RawStep {
    type: 'same' | 'removed' | 'added'
    leftIdx: number | null // 0-based
    rightIdx: number | null // 0-based
  }

  const steps: RawStep[] = []
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (normalizeLine(aLines[i]!) === normalizeLine(bLines[j]!)) {
      steps.push({ type: 'same', leftIdx: i, rightIdx: j })
      i++
      j++
    } else if (dp[i + 1]![j]! >= dp[i]![j + 1]!) {
      steps.push({ type: 'removed', leftIdx: i, rightIdx: null })
      i++
    } else {
      steps.push({ type: 'added', leftIdx: null, rightIdx: j })
      j++
    }
  }
  while (i < n) {
    steps.push({ type: 'removed', leftIdx: i++, rightIdx: null })
  }
  while (j < m) {
    steps.push({ type: 'added', leftIdx: null, rightIdx: j++ })
  }

  // Group into SameBlocks & ChangeHunks
  const blocks: DiffBlock[] = []
  let hunkCounter = 0
  let curLeftPos = 0
  let curRightPos = 0

  let idx = 0
  while (idx < steps.length) {
    const step = steps[idx]!
    if (step.type === 'same') {
      const sameRows: PairedDiffRow[] = []
      const blockStart = idx
      while (idx < steps.length && steps[idx]!.type === 'same') {
        const s = steps[idx]!
        const lIdx = s.leftIdx!
        const rIdx = s.rightIdx!
        const lText = aLines[lIdx] ?? ''
        const rText = bLines[rIdx] ?? ''
        sameRows.push({
          rowKey: `same-${lIdx}-${rIdx}`,
          kind: 'same',
          leftLineNo: lIdx + 1,
          rightLineNo: rIdx + 1,
          leftText: lText,
          rightText: rText,
          leftSegments: [{ text: lText, changed: false }],
          rightSegments: [{ text: rText, changed: false }]
        })
        curLeftPos = lIdx + 1
        curRightPos = rIdx + 1
        idx++
      }
      blocks.push({
        blockType: 'same',
        id: `same-block-${blockStart}`,
        rows: sameRows
      })
    } else {
      // Collect all contiguous removed & added steps into one ChangeHunk
      const removedIndices: number[] = []
      const addedIndices: number[] = []
      const hunkStartLeft = curLeftPos
      const hunkStartRight = curRightPos

      while (idx < steps.length && steps[idx]!.type !== 'same') {
        const s = steps[idx]!
        if (s.type === 'removed' && s.leftIdx !== null) {
          removedIndices.push(s.leftIdx)
          curLeftPos = s.leftIdx + 1
        } else if (s.type === 'added' && s.rightIdx !== null) {
          addedIndices.push(s.rightIdx)
          curRightPos = s.rightIdx + 1
        }
        idx++
      }

      hunkCounter++
      const maxLen = Math.max(removedIndices.length, addedIndices.length)
      const pairedRows: PairedDiffRow[] = []

      for (let k = 0; k < maxLen; k++) {
        const lIdx = removedIndices[k] ?? null
        const rIdx = addedIndices[k] ?? null
        const lText = lIdx !== null ? (aLines[lIdx] ?? '') : ''
        const rText = rIdx !== null ? (bLines[rIdx] ?? '') : ''

        if (lIdx !== null && rIdx !== null) {
          const { leftSegments, rightSegments } = computeInlineDiff(lText, rText)
          pairedRows.push({
            rowKey: `hunk-${hunkCounter}-pair-${k}`,
            kind: 'modified',
            leftLineNo: lIdx + 1,
            rightLineNo: rIdx + 1,
            leftText: lText,
            rightText: rText,
            leftSegments,
            rightSegments
          })
        } else if (lIdx !== null) {
          pairedRows.push({
            rowKey: `hunk-${hunkCounter}-rem-${k}`,
            kind: 'removed',
            leftLineNo: lIdx + 1,
            rightLineNo: null,
            leftText: lText,
            rightText: '',
            leftSegments: [{ text: lText, changed: true }],
            rightSegments: []
          })
        } else if (rIdx !== null) {
          pairedRows.push({
            rowKey: `hunk-${hunkCounter}-add-${k}`,
            kind: 'added',
            leftLineNo: null,
            rightLineNo: rIdx + 1,
            leftText: '',
            rightText: rText,
            leftSegments: [],
            rightSegments: [{ text: rText, changed: true }]
          })
        }
      }

      blocks.push({
        blockType: 'hunk',
        id: `hunk-${hunkCounter}`,
        hunkIndex: hunkCounter,
        leftStartIdx: removedIndices.length > 0 ? removedIndices[0]! : hunkStartLeft,
        leftCount: removedIndices.length,
        leftRawLines: removedIndices.map((li) => aLines[li] ?? ''),
        rightStartIdx: addedIndices.length > 0 ? addedIndices[0]! : hunkStartRight,
        rightCount: addedIndices.length,
        rightRawLines: addedIndices.map((ri) => bLines[ri] ?? ''),
        rows: pairedRows
      })
    }
  }

  return blocks
})

const allHunks = computed<ChangeHunk[]>(() =>
  diffBlocks.value.filter((b): b is ChangeHunk => b.blockType === 'hunk')
)

const totalHunks = computed(() => allHunks.value.length)

// Keep activeHunkIndex valid when hunks change, without auto-opening when null
watch(totalHunks, (newTotal) => {
  if (newTotal === 0) {
    activeHunkIndex.value = null
  } else if (activeHunkIndex.value !== null && activeHunkIndex.value > newTotal) {
    activeHunkIndex.value = newTotal
  }
})

const diffStats = computed(() => {
  let added = 0
  let removed = 0
  let unchanged = 0
  for (const block of diffBlocks.value) {
    if (block.blockType === 'same') {
      unchanged += block.rows.length
    } else {
      removed += block.leftCount
      added += block.rightCount
    }
  }
  return { added, removed, unchanged }
})

const searchKeyword = computed(() => store.subInputQuery.trim().toLowerCase())

function rowMatchesSearch(row: PairedDiffRow): boolean {
  const q = searchKeyword.value
  if (!q) return true
  return row.leftText.toLowerCase().includes(q) || row.rightText.toLowerCase().includes(q)
}

/**
 * Determine visible rows for a SameBlock (supports context folding & search filtering)
 */
function getSameBlockDisplay(block: SameBlock): {
  topRows: PairedDiffRow[]
  foldedCount: number
  bottomRows: PairedDiffRow[]
} {
  if (onlyShowChanges.value) {
    return { topRows: [], foldedCount: 0, bottomRows: [] }
  }
  const filtered = searchKeyword.value ? block.rows.filter(rowMatchesSearch) : block.rows
  if (
    !collapseUnchanged.value ||
    searchKeyword.value ||
    expandedSameBlockIds.value.has(block.id) ||
    filtered.length < MIN_FOLDABLE_SAME_LINES
  ) {
    return { topRows: filtered, foldedCount: 0, bottomRows: [] }
  }
  const topRows = filtered.slice(0, CONTEXT_LINES)
  const bottomRows = filtered.slice(filtered.length - CONTEXT_LINES)
  const foldedCount = filtered.length - CONTEXT_LINES * 2
  return { topRows, foldedCount, bottomRows }
}

function expandSameBlock(blockId: string) {
  const next = new Set(expandedSameBlockIds.value)
  next.add(blockId)
  expandedSameBlockIds.value = next
}

/**
 * Select and scroll to a specific ChangeHunk (1..totalHunks)
 */
function selectAndScrollToHunk(hunkIdx: number) {
  if (totalHunks.value === 0) return
  const clamped = ((hunkIdx - 1 + totalHunks.value) % totalHunks.value) + 1
  activeHunkIndex.value = clamped
  nextTick(() => {
    const container = diffScrollContainerRef.value
    const el = container?.querySelector(`[data-hunk-index="${clamped}"]`) as HTMLElement | null
    if (el) {
      el.scrollIntoView({ block: 'center', behavior: 'instant' as ScrollBehavior })
    }
  })
}

function gotoPrevHunk() {
  if (totalHunks.value === 0) return
  const cur = activeHunkIndex.value ?? 1
  selectAndScrollToHunk(cur - 1)
}

function gotoNextHunk() {
  if (totalHunks.value === 0) return
  const cur = activeHunkIndex.value ?? 0
  selectAndScrollToHunk(cur + 1)
}

/**
 * Merge a ChangeHunk from Left -> Right (`合并 >`)
 */
function mergeHunkToRight(hunk: ChangeHunk) {
  undoStack.value.push({
    leftText: leftText.value,
    rightText: rightText.value,
    description: `将更改 #${hunk.hunkIndex} 合并至右侧`
  })
  const nextRight = rightLinesArray.value.slice()
  nextRight.splice(hunk.rightStartIdx, hunk.rightCount, ...hunk.leftRawLines)
  rightText.value = nextRight.join('\n')
  activeHunkIndex.value = null
  store.showToast(`已将更改 #${hunk.hunkIndex} 合并至右侧`)
}

/**
 * Merge a ChangeHunk from Right -> Left (`< 合并`)
 */
function mergeHunkToLeft(hunk: ChangeHunk) {
  undoStack.value.push({
    leftText: leftText.value,
    rightText: rightText.value,
    description: `将更改 #${hunk.hunkIndex} 合并至左侧`
  })
  const nextLeft = leftLinesArray.value.slice()
  nextLeft.splice(hunk.leftStartIdx, hunk.leftCount, ...hunk.rightRawLines)
  leftText.value = nextLeft.join('\n')
  activeHunkIndex.value = null
  store.showToast(`已将更改 #${hunk.hunkIndex} 合并至左侧`)
}

function undoLastMerge() {
  const snap = undoStack.value.pop()
  if (!snap) return
  leftText.value = snap.leftText
  rightText.value = snap.rightText
  store.showToast(`已撤销：${snap.description}`, 'info')
}

function swapSides() {
  undoStack.value.push({
    leftText: leftText.value,
    rightText: rightText.value,
    description: '交换左右文本'
  })
  const tmp = leftText.value
  leftText.value = rightText.value
  rightText.value = tmp
  store.showToast('已交换左右两侧对比文本')
}

function clearBothSides() {
  undoStack.value.push({
    leftText: leftText.value,
    rightText: rightText.value,
    description: '清空双栏文本'
  })
  leftText.value = ''
  rightText.value = ''
  viewMode.value = 'edit'
  store.showToast('已清空左右对比文本，可直接输入或粘贴新内容')
}

async function copySideText(side: 'left' | 'right') {
  const text = side === 'left' ? leftText.value : rightText.value
  await navigator.clipboard.writeText(text)
  store.showToast(side === 'left' ? '已复制左侧全部文本' : '已复制右侧全部文本')
}

async function pasteToSide(side: 'left' | 'right') {
  try {
    const text = await navigator.clipboard.readText()
    if (!text) {
      store.showToast('剪贴板为空', 'info')
      return
    }
    if (side === 'left') {
      leftText.value = text
      store.showToast('已将剪贴板内容粘贴至左侧（文本 1）')
    } else {
      rightText.value = text
      store.showToast('已将剪贴板内容粘贴至右侧（文本 2）')
    }
  } catch {
    store.showToast('无法读取剪贴板，请在输入框内按 ⌘V 粘贴', 'warning')
  }
}

function handlePluginKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key.toLowerCase() === 'z') {
    const target = e.target as HTMLElement | null
    const isInput = target && (target.tagName === 'TEXTAREA' || target.tagName === 'INPUT')
    if (!isInput && undoStack.value.length > 0) {
      e.preventDefault()
      e.stopPropagation()
      undoLastMerge()
    }
  } else if (e.altKey && e.key === 'ArrowDown') {
    e.preventDefault()
    gotoNextHunk()
  } else if (e.altKey && e.key === 'ArrowUp') {
    e.preventDefault()
    gotoPrevHunk()
  }
}
</script>

<template>
  <div
    class="flex flex-col flex-1 h-full min-h-[500px] select-none overflow-hidden"
    :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
  >
    <!-- =================================================================== -->
    <!-- Top Main Diff Toolbar                                               -->
    <!-- =================================================================== -->
    <div
      class="flex items-center justify-between px-4 py-2 border-b text-xs shrink-0 gap-2"
      :class="
        store.isDark
          ? 'bg-obsidian-950/60 border-slate-800/80'
          : 'bg-slate-50/90 border-slate-200/80'
      "
    >
      <div class="flex items-center gap-2 flex-wrap">
        <!-- Mode Segmented Control -->
        <div
          class="flex items-center p-0.5 rounded-xl border"
          :class="store.isDark ? 'bg-slate-900 border-slate-800' : 'bg-slate-200/60 border-slate-200'"
        >
          <button
            type="button"
            class="px-2.5 py-1 rounded-lg font-medium cursor-pointer transition-all"
            :class="
              viewMode === 'split'
                ? 'bg-indigo-600 text-white shadow-2xs'
                : 'text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200'
            "
            @click="viewMode = 'split'"
          >
            高亮差异对比
          </button>
          <button
            type="button"
            class="px-2.5 py-1 rounded-lg font-medium cursor-pointer transition-all"
            :class="
              viewMode === 'edit'
                ? 'bg-indigo-600 text-white shadow-2xs'
                : 'text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200'
            "
            @click="viewMode = 'edit'"
          >
            编辑源文本
          </button>
        </div>

        <!-- Global Hunk Prev / Next Navigator -->
        <div
          v-if="viewMode === 'split' && totalHunks > 0"
          class="flex items-center gap-1 px-2 py-0.5 rounded-lg border font-mono text-[11px]"
          :class="
            store.isDark
              ? 'bg-slate-900/90 border-slate-800 text-slate-300'
              : 'bg-white border-slate-200/90 text-slate-700'
          "
        >
          <span>
            {{ activeHunkIndex ? `更改 ${activeHunkIndex}/${totalHunks}` : `共 ${totalHunks} 处差异` }}
          </span>
          <button
            type="button"
            title="上一个差异块 (Option+↑)"
            class="px-1 hover:text-indigo-500 cursor-pointer"
            @click="gotoPrevHunk"
          >
            ↑
          </button>
          <button
            type="button"
            title="下一个差异块 (Option+↓)"
            class="px-1 hover:text-indigo-500 cursor-pointer"
            @click="gotoNextHunk"
          >
            ↓
          </button>
        </div>

        <!-- Undo Merge Button -->
        <button
          v-if="undoStack.length > 0"
          type="button"
          title="撤销上一次合并或修改 (⌘Z)"
          class="flex items-center gap-1 px-2 py-1 rounded-lg border text-[11px] font-medium cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-indigo-500/15 border-indigo-500/40 text-indigo-300 hover:bg-indigo-500/25'
              : 'bg-indigo-50 border-indigo-200 text-indigo-600 hover:bg-indigo-100'
          "
          @click="undoLastMerge"
        >
          <IconRenderer name="Undo2" :size="12" />
          <span>撤销 ({{ undoStack.length }})</span>
        </button>

        <button
          type="button"
          class="btn-secondary !px-2 !py-1 !rounded-lg"
          @click="swapSides"
        >
          <IconRenderer name="ArrowLeftRight" :size="12" />
          <span>交换左右</span>
        </button>

        <button
          type="button"
          class="btn-secondary !px-2 !py-1 !rounded-lg hover:!text-rose-500"
          title="清空左右两侧文本并进入编辑模式"
          @click="clearBothSides"
        >
          <IconRenderer name="Trash2" :size="12" />
          <span>清空</span>
        </button>

        <!-- Checkboxes: Word Wrap, Collapse Unchanged, Ignore Whitespace, Only Changes -->
        <label class="flex items-center gap-1 ml-1 cursor-pointer text-slate-500 dark:text-slate-400">
          <input v-model="wordWrap" type="checkbox" class="accent-indigo-600 rounded cursor-pointer" />
          <span>自动换行</span>
        </label>

        <label class="flex items-center gap-1 cursor-pointer text-slate-500 dark:text-slate-400">
          <input
            v-model="collapseUnchanged"
            type="checkbox"
            class="accent-indigo-600 rounded cursor-pointer"
          />
          <span>折叠相同行</span>
        </label>

        <label class="flex items-center gap-1 cursor-pointer text-slate-500 dark:text-slate-400">
          <input
            v-model="ignoreWhitespace"
            type="checkbox"
            class="accent-indigo-600 rounded cursor-pointer"
          />
          <span>忽略空白</span>
        </label>

        <label class="flex items-center gap-1 cursor-pointer text-slate-500 dark:text-slate-400">
          <input
            v-model="onlyShowChanges"
            type="checkbox"
            class="accent-indigo-600 rounded cursor-pointer"
          />
          <span>仅看差异</span>
        </label>
      </div>
    </div>

    <!-- =================================================================== -->
    <!-- Split Diff Header Bar (Left & Right Summary + One-Click Copy All)   -->
    <!-- =================================================================== -->
    <div
      v-if="viewMode === 'split'"
      class="grid grid-cols-2 divide-x border-b text-[11px] shrink-0"
      :class="
        store.isDark
          ? 'bg-slate-900/70 divide-slate-800 border-slate-800 text-slate-300'
          : 'bg-slate-100/80 divide-slate-200 border-slate-200/90 text-slate-600'
      "
    >
      <!-- Left Pane Subheader -->
      <div class="flex items-center justify-between px-3.5 py-1.5">
        <div class="flex items-center gap-2">
          <span
            class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded font-mono font-semibold"
            :class="
              store.isDark
                ? 'bg-rose-500/15 text-rose-300 border border-rose-500/30'
                : 'bg-rose-50 text-rose-600 border border-rose-200'
            "
          >
            - {{ diffStats.removed }} 删除
          </span>
          <span class="font-mono text-slate-400">{{ leftLinesArray.length }} 行</span>
        </div>
        <button
          type="button"
          class="flex items-center gap-1 px-2 py-0.5 rounded border cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-slate-800 border-slate-700 hover:text-indigo-300 hover:border-indigo-500/40'
              : 'bg-white border-slate-200 hover:text-indigo-600 hover:border-indigo-300'
          "
          @click="copySideText('left')"
        >
          <IconRenderer name="Copy" :size="11" />
          <span>全部复制</span>
        </button>
      </div>

      <!-- Right Pane Subheader -->
      <div class="flex items-center justify-between px-3.5 py-1.5">
        <div class="flex items-center gap-2">
          <span
            class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded font-mono font-semibold"
            :class="
              store.isDark
                ? 'bg-emerald-500/15 text-emerald-300 border border-emerald-500/30'
                : 'bg-emerald-50 text-emerald-700 border border-emerald-200'
            "
          >
            + {{ diffStats.added }} 添加
          </span>
          <span class="font-mono text-slate-400">{{ rightLinesArray.length }} 行</span>
        </div>
        <button
          type="button"
          class="flex items-center gap-1 px-2 py-0.5 rounded border cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-slate-800 border-slate-700 hover:text-indigo-300 hover:border-indigo-500/40'
              : 'bg-white border-slate-200 hover:text-indigo-600 hover:border-indigo-300'
          "
          @click="copySideText('right')"
        >
          <IconRenderer name="Copy" :size="11" />
          <span>全部复制</span>
        </button>
      </div>
    </div>

    <!-- =================================================================== -->
    <!-- Mode A: Side-by-Side Aligned Hunk Diff View + Interactive Merge     -->
    <!-- =================================================================== -->
    <div
      v-if="viewMode === 'split'"
      ref="diffScrollContainerRef"
      class="flex-1 overflow-y-auto font-mono text-[12px] leading-[22px] select-text px-2 py-1.5"
    >
      <div
        v-if="totalHunks === 0 && onlyShowChanges"
        class="py-16 text-center text-xs text-slate-400 font-sans"
      >
        左右两侧文本完全相同，未检测到任何差异行
      </div>

      <template v-for="block in diffBlocks" :key="block.id">
        <!-- 1. Unchanged SameBlock (with optional context folding) -->
        <template v-if="block.blockType === 'same'">
          <div
            v-for="row in getSameBlockDisplay(block).topRows"
            :key="row.rowKey"
            class="grid grid-cols-2 divide-x border-b"
            :class="
              store.isDark
                ? 'divide-slate-800/70 border-slate-800/30'
                : 'divide-slate-200/80 border-slate-100'
            "
          >
            <!-- Left Same Cell -->
            <div class="flex items-start min-w-0">
              <span
                class="w-9 px-1.5 text-right select-none text-[11px] shrink-0 opacity-45 border-r"
                :class="store.isDark ? 'border-slate-800' : 'border-slate-200'"
              >
                {{ row.leftLineNo ?? '' }}
              </span>
              <span class="w-4 shrink-0"></span>
              <pre
                class="px-1.5 m-0 font-mono flex-1 min-w-0"
                :class="wordWrap ? 'whitespace-pre-wrap break-all' : 'whitespace-pre overflow-x-auto'"
              >{{ row.leftText }}</pre>
            </div>

            <!-- Right Same Cell -->
            <div class="flex items-start min-w-0">
              <span
                class="w-9 px-1.5 text-right select-none text-[11px] shrink-0 opacity-45 border-r"
                :class="store.isDark ? 'border-slate-800' : 'border-slate-200'"
              >
                {{ row.rightLineNo ?? '' }}
              </span>
              <span class="w-4 shrink-0"></span>
              <pre
                class="px-1.5 m-0 font-mono flex-1 min-w-0"
                :class="wordWrap ? 'whitespace-pre-wrap break-all' : 'whitespace-pre overflow-x-auto'"
              >{{ row.rightText }}</pre>
            </div>
          </div>

          <!-- Context Folded Expander Pill -->
          <div
            v-if="getSameBlockDisplay(block).foldedCount > 0"
            class="my-1 flex items-center justify-center py-1 rounded-lg border text-[11px] font-sans cursor-pointer transition-colors"
            :class="
              store.isDark
                ? 'bg-slate-900/70 border-slate-800 text-slate-400 hover:text-indigo-300 hover:border-indigo-500/40'
                : 'bg-slate-100/90 border-slate-200/80 text-slate-500 hover:text-indigo-600 hover:border-indigo-300'
            "
            @click="expandSameBlock(block.id)"
          >
            <IconRenderer name="ChevronsUpDown" :size="13" class-name="mr-1" />
            <span>展开中间隐藏的 {{ getSameBlockDisplay(block).foldedCount }} 行相同内容</span>
          </div>

          <div
            v-for="row in getSameBlockDisplay(block).bottomRows"
            :key="row.rowKey"
            class="grid grid-cols-2 divide-x border-b"
            :class="
              store.isDark
                ? 'divide-slate-800/70 border-slate-800/30'
                : 'divide-slate-200/80 border-slate-100'
            "
          >
            <!-- Left Same Cell -->
            <div class="flex items-start min-w-0">
              <span
                class="w-9 px-1.5 text-right select-none text-[11px] shrink-0 opacity-45 border-r"
                :class="store.isDark ? 'border-slate-800' : 'border-slate-200'"
              >
                {{ row.leftLineNo ?? '' }}
              </span>
              <span class="w-4 shrink-0"></span>
              <pre
                class="px-1.5 m-0 font-mono flex-1 min-w-0"
                :class="wordWrap ? 'whitespace-pre-wrap break-all' : 'whitespace-pre overflow-x-auto'"
              >{{ row.leftText }}</pre>
            </div>

            <!-- Right Same Cell -->
            <div class="flex items-start min-w-0">
              <span
                class="w-9 px-1.5 text-right select-none text-[11px] shrink-0 opacity-45 border-r"
                :class="store.isDark ? 'border-slate-800' : 'border-slate-200'"
              >
                {{ row.rightLineNo ?? '' }}
              </span>
              <span class="w-4 shrink-0"></span>
              <pre
                class="px-1.5 m-0 font-mono flex-1 min-w-0"
                :class="wordWrap ? 'whitespace-pre-wrap break-all' : 'whitespace-pre overflow-x-auto'"
              >{{ row.rightText }}</pre>
            </div>
          </div>
        </template>

        <!-- 2. Interactive ChangeHunk (Diffchecker-style Selectable Block + Merge Left/Right) -->
        <div
          v-else
          :data-hunk-index="block.hunkIndex"
          class="transition-all cursor-pointer overflow-hidden"
          :class="
            activeHunkIndex === block.hunkIndex
              ? store.isDark
                ? 'my-1.5 rounded-xl border border-indigo-500/80 ring-2 ring-indigo-500/25 bg-slate-900/40 shadow-lg'
                : 'my-1.5 rounded-xl border border-indigo-400 ring-2 ring-indigo-500/20 bg-white shadow-md'
              : store.isDark
                ? 'border-b border-slate-800/40 hover:ring-1 hover:ring-indigo-500/40'
                : 'border-b border-slate-100 hover:ring-1 hover:ring-indigo-300'
          "
          @click="
            activeHunkIndex = activeHunkIndex === block.hunkIndex ? null : block.hunkIndex
          "
        >
          <!-- Active Hunk Top Control Bar (Shown ONLY when this hunk is clicked/selected) -->
          <div
            v-if="activeHunkIndex === block.hunkIndex"
            class="flex items-center justify-between px-3 py-1.5 border-b font-sans text-xs"
            :class="
              store.isDark
                ? 'bg-[#141D32] border-slate-800 text-slate-200'
                : 'bg-slate-50 border-slate-200/80 text-slate-700'
            "
            @click.stop
          >
            <div class="flex items-center gap-2">
              <span class="font-semibold">
                更改 <span class="font-mono">{{ block.hunkIndex }}</span> /
                <span class="font-mono">{{ totalHunks }}</span>
              </span>

              <button
                type="button"
                class="flex items-center gap-1 px-2 py-0.5 rounded-md border text-[11px] cursor-pointer transition-colors"
                :class="
                  store.isDark
                    ? 'bg-slate-800 border-slate-700 hover:bg-slate-700 text-slate-200'
                    : 'bg-white border-slate-200/90 hover:bg-slate-100 text-slate-700'
                "
                @click.stop="gotoPrevHunk"
              >
                <span>↑ 上一个</span>
              </button>

              <button
                type="button"
                class="flex items-center gap-1 px-2 py-0.5 rounded-md border text-[11px] cursor-pointer transition-colors"
                :class="
                  store.isDark
                    ? 'bg-slate-800 border-slate-700 hover:bg-slate-700 text-slate-200'
                    : 'bg-white border-slate-200/90 hover:bg-slate-100 text-slate-700'
                "
                @click.stop="gotoNextHunk"
              >
                <span>↓ 下一个</span>
              </button>
            </div>

            <div class="flex items-center gap-2 text-[11px] text-slate-400">
              <span>点击下方按钮可将该处差异向右或向左合并</span>
              <button
                type="button"
                title="收起合并栏 (Esc)"
                class="p-0.5 rounded hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
                @click.stop="activeHunkIndex = null"
              >
                <IconRenderer name="X" :size="13" />
              </button>
            </div>
          </div>

          <!-- Paired Diff Rows inside this Hunk -->
          <div
            v-for="row in block.rows.filter(rowMatchesSearch)"
            :key="row.rowKey"
            class="grid grid-cols-2 divide-x border-b last:border-b-0"
            :class="
              store.isDark
                ? 'divide-slate-800/70 border-slate-800/40'
                : 'divide-slate-200/80 border-slate-100'
            "
          >
            <!-- Left Side (Removed / Modified Original) -->
            <div
              class="flex items-start min-w-0"
              :class="
                row.leftLineNo !== null
                  ? store.isDark
                    ? 'bg-rose-500/12 text-rose-100'
                    : 'bg-rose-50/75 text-rose-950'
                  : store.isDark
                    ? 'bg-slate-950/30'
                    : 'bg-slate-50/60'
              "
            >
              <span
                class="w-9 px-1.5 text-right select-none text-[11px] shrink-0 opacity-55 border-r"
                :class="store.isDark ? 'border-slate-800' : 'border-slate-200'"
              >
                {{ row.leftLineNo ?? '' }}
              </span>
              <span class="w-4 text-center select-none font-bold text-rose-500 shrink-0">
                {{ row.leftLineNo !== null ? '-' : '' }}
              </span>
              <pre
                v-if="row.leftLineNo !== null"
                class="px-1.5 m-0 font-mono flex-1 min-w-0"
                :class="wordWrap ? 'whitespace-pre-wrap break-all' : 'whitespace-pre overflow-x-auto'"
              ><template v-for="(seg, sIdx) in row.leftSegments" :key="sIdx"><mark
                v-if="seg.changed"
                class="rounded-xs px-0.5 font-semibold"
                :class="
                  store.isDark
                    ? 'bg-rose-500/40 text-rose-100'
                    : 'bg-rose-200/90 text-rose-950'
                "
              >{{ seg.text }}</mark><template v-else>{{ seg.text }}</template></template></pre>
            </div>

            <!-- Right Side (Added / Modified New) -->
            <div
              class="flex items-start min-w-0"
              :class="
                row.rightLineNo !== null
                  ? store.isDark
                    ? 'bg-emerald-500/12 text-emerald-100'
                    : 'bg-emerald-50/75 text-emerald-950'
                  : store.isDark
                    ? 'bg-slate-950/30'
                    : 'bg-slate-50/60'
              "
            >
              <span
                class="w-9 px-1.5 text-right select-none text-[11px] shrink-0 opacity-55 border-r"
                :class="store.isDark ? 'border-slate-800' : 'border-slate-200'"
              >
                {{ row.rightLineNo ?? '' }}
              </span>
              <span class="w-4 text-center select-none font-bold text-emerald-500 shrink-0">
                {{ row.rightLineNo !== null ? '+' : '' }}
              </span>
              <pre
                v-if="row.rightLineNo !== null"
                class="px-1.5 m-0 font-mono flex-1 min-w-0"
                :class="wordWrap ? 'whitespace-pre-wrap break-all' : 'whitespace-pre overflow-x-auto'"
              ><template v-for="(seg, sIdx) in row.rightSegments" :key="sIdx"><mark
                v-if="seg.changed"
                class="rounded-xs px-0.5 font-semibold"
                :class="
                  store.isDark
                    ? 'bg-emerald-500/40 text-emerald-100'
                    : 'bg-emerald-200/90 text-emerald-950'
                "
              >{{ seg.text }}</mark><template v-else>{{ seg.text }}</template></template></pre>
            </div>
          </div>

          <!-- Active Hunk Bottom Merge Action Bar (`合并 >` | `< 合并`) -->
          <div
            v-if="activeHunkIndex === block.hunkIndex"
            class="flex items-center justify-center gap-2.5 py-2 px-3 border-t font-sans text-xs"
            :class="
              store.isDark
                ? 'bg-[#111827]/95 border-slate-800'
                : 'bg-slate-50/90 border-slate-200/80'
            "
          >
            <!-- Merge Left -> Right (`合并 >`) -->
            <button
              type="button"
              title="将左侧此差异块的内容覆盖合并到右侧"
              class="flex items-center gap-1.5 px-3 py-1 rounded-lg font-semibold text-white bg-rose-600 hover:bg-rose-500 active:bg-rose-700 cursor-pointer shadow-xs transition-colors"
              @click.stop="mergeHunkToRight(block)"
            >
              <span>合并</span>
              <span class="font-mono font-bold">&gt;</span>
            </button>

            <span class="text-slate-400 text-[10px]">•</span>

            <!-- Merge Right -> Left (`< 合并`) -->
            <button
              type="button"
              title="将右侧此差异块的内容覆盖合并到左侧"
              class="flex items-center gap-1.5 px-3 py-1 rounded-lg font-semibold text-white bg-emerald-600 hover:bg-emerald-500 active:bg-emerald-700 cursor-pointer shadow-xs transition-colors"
              @click.stop="mergeHunkToLeft(block)"
            >
              <span class="font-mono font-bold">&lt;</span>
              <span>合并</span>
            </button>
          </div>
        </div>
      </template>
    </div>

    <!-- =================================================================== -->
    <!-- Mode B: Dual Raw Text Editors                                       -->
    <!-- =================================================================== -->
    <div
      v-else
      class="flex-1 grid grid-cols-2 divide-x min-h-0"
      :class="store.isDark ? 'divide-slate-800' : 'divide-slate-200'"
    >
      <div class="flex flex-col min-h-0">
        <div
          class="flex items-center justify-between px-3 py-1.5 text-[11px] font-mono text-slate-400 border-b border-inherit"
        >
          <span>原始文本 1 (Original · {{ leftLinesArray.length }} 行)</span>
          <div class="flex items-center gap-2 font-sans">
            <button
              type="button"
              class="text-indigo-500 hover:underline cursor-pointer flex items-center gap-1"
              @click="pasteToSide('left')"
            >
              <IconRenderer name="ClipboardPaste" :size="11" />
              <span>粘贴</span>
            </button>
            <button
              type="button"
              class="text-slate-400 hover:text-indigo-500 cursor-pointer"
              @click="copySideText('left')"
            >
              复制
            </button>
            <button
              v-if="leftText"
              type="button"
              class="text-slate-400 hover:text-rose-500 cursor-pointer"
              @click="leftText = ''"
            >
              清空
            </button>
          </div>
        </div>
        <textarea
          v-model="leftText"
          spellcheck="false"
          placeholder="在此输入或粘贴第一段对比文本..."
          aria-label="左侧原始对比文本"
          class="flex-1 w-full p-3 bg-transparent font-mono text-[12.5px] leading-relaxed resize-none outline-none"
          :class="[
            store.isDark ? 'text-slate-100' : 'text-slate-800',
            wordWrap ? 'whitespace-pre-wrap break-all' : 'whitespace-pre overflow-x-auto'
          ]"
        ></textarea>
      </div>
      <div class="flex flex-col min-h-0">
        <div
          class="flex items-center justify-between px-3 py-1.5 text-[11px] font-mono text-slate-400 border-b border-inherit"
        >
          <span>变更后文本 2 (Modified · {{ rightLinesArray.length }} 行)</span>
          <div class="flex items-center gap-2 font-sans">
            <button
              type="button"
              class="text-indigo-500 hover:underline cursor-pointer flex items-center gap-1"
              @click="pasteToSide('right')"
            >
              <IconRenderer name="ClipboardPaste" :size="11" />
              <span>粘贴</span>
            </button>
            <button
              type="button"
              class="text-slate-400 hover:text-indigo-500 cursor-pointer"
              @click="copySideText('right')"
            >
              复制
            </button>
            <button
              v-if="rightText"
              type="button"
              class="text-slate-400 hover:text-rose-500 cursor-pointer"
              @click="rightText = ''"
            >
              清空
            </button>
          </div>
        </div>
        <textarea
          v-model="rightText"
          spellcheck="false"
          placeholder="在此输入或粘贴第二段对比文本..."
          aria-label="右侧变更后对比文本"
          class="flex-1 w-full p-3 bg-transparent font-mono text-[12.5px] leading-relaxed resize-none outline-none"
          :class="[
            store.isDark ? 'text-slate-100' : 'text-slate-800',
            wordWrap ? 'whitespace-pre-wrap break-all' : 'whitespace-pre overflow-x-auto'
          ]"
        ></textarea>
      </div>
    </div>
  </div>
</template>
