<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import IconRenderer from '@/components/IconRenderer.vue'

interface DiffRow {
  type: 'same' | 'added' | 'removed' | 'modified'
  leftLineNo: number | null
  rightLineNo: number | null
  leftText: string
  rightText: string
}

const DRAFT_STORAGE_KEY = 'omnibox:live-draft:diff-viewer'

const DEFAULT_LEFT_TEXT = `{
  "service": "payment-gateway",
  "replicas": 3,
  "timeoutMs": 3000,
  "features": {
    "retry": true,
    "circuitBreaker": false
  },
  "region": "ap-east-1"
}`

const DEFAULT_RIGHT_TEXT = `{
  "service": "payment-gateway-v2",
  "replicas": 6,
  "timeoutMs": 1500,
  "features": {
    "retry": true,
    "circuitBreaker": true,
    "rateLimit": 5000
  },
  "region": "ap-east-1"
}`

const store = useLauncherStore()

const leftText = ref(DEFAULT_LEFT_TEXT)
const rightText = ref(DEFAULT_RIGHT_TEXT)
const viewMode = ref<'split' | 'edit'>('split')
const ignoreWhitespace = ref(false)
const onlyShowChanges = ref(false)

onMounted(() => {
  try {
    const rawDraft = localStorage.getItem(DRAFT_STORAGE_KEY)
    if (rawDraft) {
      const parsed = JSON.parse(rawDraft) as {
        leftText?: string
        rightText?: string
        viewMode?: 'split' | 'edit'
        ignoreWhitespace?: boolean
        onlyShowChanges?: boolean
      }
      if (typeof parsed.leftText === 'string') leftText.value = parsed.leftText
      if (typeof parsed.rightText === 'string') rightText.value = parsed.rightText
      if (parsed.viewMode === 'split' || parsed.viewMode === 'edit') {
        viewMode.value = parsed.viewMode
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
})

watch([leftText, rightText, viewMode, ignoreWhitespace, onlyShowChanges], () => {
  try {
    localStorage.setItem(
      DRAFT_STORAGE_KEY,
      JSON.stringify({
        leftText: leftText.value,
        rightText: rightText.value,
        viewMode: viewMode.value,
        ignoreWhitespace: ignoreWhitespace.value,
        onlyShowChanges: onlyShowChanges.value
      })
    )
  } catch {
    // ignore storage quota errors
  }
})

function normalize(line: string): string {
  return ignoreWhitespace.value ? line.trim().replace(/\s+/g, ' ') : line
}

// Compute line-by-line LCS diff
const diffRows = computed<DiffRow[]>(() => {
  const aLines = leftText.value.split('\n')
  const bLines = rightText.value.split('\n')
  const n = aLines.length
  const m = bLines.length

  const dp: number[][] = Array.from({ length: n + 1 }, () => Array(m + 1).fill(0))
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      if (normalize(aLines[i]) === normalize(bLines[j])) {
        dp[i][j] = 1 + dp[i + 1][j + 1]
      } else {
        dp[i][j] = Math.max(dp[i + 1][j], dp[i][j + 1])
      }
    }
  }

  const rows: DiffRow[] = []
  let i = 0
  let j = 0
  let leftNo = 1
  let rightNo = 1

  while (i < n && j < m) {
    if (normalize(aLines[i]) === normalize(bLines[j])) {
      rows.push({
        type: 'same',
        leftLineNo: leftNo++,
        rightLineNo: rightNo++,
        leftText: aLines[i],
        rightText: bLines[j]
      })
      i++
      j++
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      rows.push({
        type: 'removed',
        leftLineNo: leftNo++,
        rightLineNo: null,
        leftText: aLines[i],
        rightText: ''
      })
      i++
    } else {
      rows.push({
        type: 'added',
        leftLineNo: null,
        rightLineNo: rightNo++,
        leftText: '',
        rightText: bLines[j]
      })
      j++
    }
  }

  while (i < n) {
    rows.push({
      type: 'removed',
      leftLineNo: leftNo++,
      rightLineNo: null,
      leftText: aLines[i++],
      rightText: ''
    })
  }
  while (j < m) {
    rows.push({
      type: 'added',
      leftLineNo: null,
      rightLineNo: rightNo++,
      leftText: '',
      rightText: bLines[j++]
    })
  }

  const q = store.subInputQuery.trim().toLowerCase()
  return rows.filter((r) => {
    if (onlyShowChanges.value && r.type === 'same') return false
    if (q) {
      return r.leftText.toLowerCase().includes(q) || r.rightText.toLowerCase().includes(q)
    }
    return true
  })
})

const diffStats = computed(() => {
  let added = 0
  let removed = 0
  let unchanged = 0
  for (const r of diffRows.value) {
    if (r.type === 'added') added++
    else if (r.type === 'removed') removed++
    else unchanged++
  }
  return { added, removed, unchanged }
})

function swapSides() {
  const tmp = leftText.value
  leftText.value = rightText.value
  rightText.value = tmp
  store.showToast('已交换左右两侧对比文本')
}

function clearBothSides() {
  leftText.value = ''
  rightText.value = ''
  viewMode.value = 'edit'
  store.showToast('已清空左右对比文本，可直接输入或粘贴新内容')
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
</script>

<template>
  <div
    class="flex flex-col flex-1 h-full min-h-[500px] select-none"
    :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
  >
    <!-- Top Diff Controls Toolbar -->
    <div
      class="flex items-center justify-between px-4 py-2 border-b text-xs"
      :class="
        store.isDark
          ? 'bg-obsidian-950/60 border-slate-800/80'
          : 'bg-slate-50/90 border-slate-200/80'
      "
    >
      <div class="flex items-center gap-2">
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

        <button
          type="button"
          class="btn-secondary !px-2.5 !py-1 !rounded-lg"
          @click="swapSides"
        >
          <IconRenderer name="ArrowLeftRight" :size="13" />
          <span>交换左右</span>
        </button>

        <button
          type="button"
          class="btn-secondary !px-2.5 !py-1 !rounded-lg hover:!text-rose-500"
          title="清空左右两侧文本并进入编辑模式"
          @click="clearBothSides"
        >
          <IconRenderer name="Trash2" :size="13" />
          <span>清空</span>
        </button>

        <label class="flex items-center gap-1.5 ml-2 cursor-pointer text-slate-500 dark:text-slate-400">
          <input v-model="ignoreWhitespace" type="checkbox" class="accent-indigo-600 rounded cursor-pointer" />
          <span>忽略空白字符</span>
        </label>

        <label class="flex items-center gap-1.5 ml-1 cursor-pointer text-slate-500 dark:text-slate-400">
          <input v-model="onlyShowChanges" type="checkbox" class="accent-indigo-600 rounded cursor-pointer" />
          <span>仅看差异行</span>
        </label>
      </div>

      <!-- Diff Counters -->
      <div class="flex items-center gap-2.5 font-mono text-[11px]">
        <span
          class="px-2 py-0.5 rounded-md border"
          :class="
            store.isDark
              ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
              : 'bg-indigo-50 text-indigo-600 border-indigo-200/80'
          "
        >
          +{{ diffStats.added }} 新增
        </span>
        <span
          class="px-2 py-0.5 rounded-md border"
          :class="
            store.isDark
              ? 'bg-rose-500/15 text-rose-300 border-rose-500/30'
              : 'bg-rose-50 text-rose-600 border-rose-200/80'
          "
        >
          -{{ diffStats.removed }} 删除
        </span>
        <span class="text-slate-400">{{ diffStats.unchanged }} 相同</span>
      </div>
    </div>

    <!-- Mode A: Side-by-Side Diff Highlight View -->
    <div
      v-if="viewMode === 'split'"
      class="flex-1 overflow-y-auto font-mono text-[12.5px] leading-[24px] select-text"
    >
      <div
        v-for="(row, idx) in diffRows"
        :key="idx"
        class="grid grid-cols-2 divide-x border-b"
        :class="
          store.isDark
            ? 'divide-slate-800/70 border-slate-800/40'
            : 'divide-slate-200 border-slate-100'
        "
      >
        <!-- Left Side (Original / Removed) -->
        <div
          class="flex min-w-0"
          :class="
            row.type === 'removed'
              ? store.isDark
                ? 'bg-rose-500/15 text-rose-200'
                : 'bg-rose-50 text-rose-900'
              : ''
          "
        >
          <span
            class="w-10 px-2 text-right select-none text-[11px] shrink-0 opacity-50 border-r"
            :class="store.isDark ? 'border-slate-800' : 'border-slate-200'"
          >
            {{ row.leftLineNo ?? '' }}
          </span>
          <span class="w-5 text-center select-none font-bold text-rose-500 shrink-0">
            {{ row.type === 'removed' ? '-' : '' }}
          </span>
          <pre class="px-2 truncate whitespace-pre">{{ row.leftText }}</pre>
        </div>

        <!-- Right Side (Modified / Added) -->
        <div
          class="flex min-w-0"
          :class="
            row.type === 'added'
              ? store.isDark
                ? 'bg-indigo-500/15 text-indigo-200'
                : 'bg-indigo-50/80 text-indigo-950'
              : ''
          "
        >
          <span
            class="w-10 px-2 text-right select-none text-[11px] shrink-0 opacity-50 border-r"
            :class="store.isDark ? 'border-slate-800' : 'border-slate-200'"
          >
            {{ row.rightLineNo ?? '' }}
          </span>
          <span class="w-5 text-center select-none font-bold text-indigo-500 shrink-0">
            {{ row.type === 'added' ? '+' : '' }}
          </span>
          <pre class="px-2 truncate whitespace-pre">{{ row.rightText }}</pre>
        </div>
      </div>
    </div>

    <!-- Mode B: Dual Raw Text Editors -->
    <div
      v-else
      class="flex-1 grid grid-cols-2 divide-x min-h-0"
      :class="store.isDark ? 'divide-slate-800' : 'divide-slate-200'"
    >
      <div class="flex flex-col min-h-0">
        <div class="flex items-center justify-between px-3 py-1.5 text-[11px] font-mono text-slate-400 border-b border-inherit">
          <span>原始文本 1 (Original)</span>
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
          :class="store.isDark ? 'text-slate-100' : 'text-slate-800'"
        ></textarea>
      </div>
      <div class="flex flex-col min-h-0">
        <div class="flex items-center justify-between px-3 py-1.5 text-[11px] font-mono text-slate-400 border-b border-inherit">
          <span>变更后文本 2 (Modified)</span>
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
          :class="store.isDark ? 'text-slate-100' : 'text-slate-800'"
        ></textarea>
      </div>
    </div>
  </div>
</template>
