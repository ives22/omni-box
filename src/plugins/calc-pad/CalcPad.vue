<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { evaluate } from 'mathjs'
import { useLauncherStore } from '@/stores/launcher'
import { kvGet, kvPut } from '@/bridge/backend'
import IconRenderer from '@/components/IconRenderer.vue'

interface CalcSheet {
  id: string
  title: string
  content: string
  updatedAt: number
}

interface EvaluatedLine {
  lineNumber: number
  raw: string
  isComment: boolean
  result: string | null
  numericValue: number | null
  error: boolean
}

const store = useLauncherStore()

const DEFAULT_SHEETS: CalcSheet[] = [
  {
    id: 'sheet-cloud-budget',
    title: '云架构与研发成本核算',
    content: `# 2026 核心集群月度预算计算稿纸
基础节点单价 = 480
节点数量 = 12
计算集群月费 = 基础节点单价 * 节点数量

带宽与CDN = 1350
数据库RDS = 2100

月度总开销 = 计算集群月费 + 带宽与CDN + 数据库RDS
年度折后总计 = 月度总开销 * 12 * 0.85

# 常用单位与进制换算
16384 MB to GB
2.5 hours to minutes
(1920 * 1080 * 4) / 1024 / 1024`,
    updatedAt: Date.now()
  },
  {
    id: 'sheet-quick',
    title: '日常随手算 & 汇率折扣',
    content: `# 商品满减与分摊计算
原价 = 1299 + 459 + 320
满减优惠 = 300
实付金额 = 原价 - 满减优惠
人均分摊 = 实付金额 / 3

# 科学计算与幂运算
2 ^ 16
sqrt(144) + 88 * 1.15`,
    updatedAt: Date.now()
  }
]

const sheets = ref<CalcSheet[]>([])
const activeSheetId = ref<string>('sheet-cloud-budget')

const activeSheet = computed(() => {
  return sheets.value.find((s) => s.id === activeSheetId.value) || sheets.value[0]
})

// Preprocess Chinese variable names into safe internal scope identifiers
function normalizeChineseVars(expr: string, varMap: Map<string, string>): string {
  return expr.replace(/[\u4e00-\u9fa5a-zA-Z_][\u4e00-\u9fa5a-zA-Z0-9_]*/g, (token) => {
    if (/^(to|in|sin|cos|tan|sqrt|log|abs|round|ceil|floor|pi|e|MB|GB|KB|TB|hours|minutes|cm|mm|km|mile|deg|rad)$/i.test(token)) {
      return token
    }
    if (/^[\u4e00-\u9fa5]/.test(token)) {
      if (!varMap.has(token)) {
        varMap.set(token, `__v_${varMap.size + 1}`)
      }
      return varMap.get(token)!
    }
    return token
  })
}

const evaluatedLines = computed<EvaluatedLine[]>(() => {
  if (!activeSheet.value) return []
  const rawLines = activeSheet.value.content.split('\n')
  const scope: Record<string, unknown> = {}
  const varMap = new Map<string, string>()
  const numericHistory: number[] = []

  return rawLines.map((line, idx) => {
    const trimmed = line.trim()
    if (!trimmed) {
      return {
        lineNumber: idx + 1,
        raw: line,
        isComment: false,
        result: null,
        numericValue: null,
        error: false
      }
    }

    if (trimmed.startsWith('#') || trimmed.startsWith('//')) {
      return {
        lineNumber: idx + 1,
        raw: line,
        isComment: true,
        result: null,
        numericValue: null,
        error: false
      }
    }

    if (/^(sum|total|合计|总计)$/i.test(trimmed)) {
      const total = numericHistory.reduce((acc, n) => acc + n, 0)
      const formatted = Number(total.toFixed(4)).toLocaleString('en-US', { maximumFractionDigits: 4 })
      return {
        lineNumber: idx + 1,
        raw: line,
        isComment: false,
        result: formatted,
        numericValue: total,
        error: false
      }
    }

    try {
      const normalized = normalizeChineseVars(trimmed, varMap)
      const val = evaluate(normalized, scope)
      if (val !== undefined && val !== null && typeof val !== 'function') {
        let numVal: number | null = null
        let displayStr = ''
        if (typeof val === 'number') {
          numVal = val
          numericHistory.push(val)
          displayStr = Number(val.toFixed(6)).toString()
        } else {
          displayStr = val.toString()
        }
        return {
          lineNumber: idx + 1,
          raw: line,
          isComment: false,
          result: displayStr,
          numericValue: numVal,
          error: false
        }
      }
    } catch {
      return {
        lineNumber: idx + 1,
        raw: line,
        isComment: false,
        result: null,
        numericValue: null,
        error: true
      }
    }

    return {
      lineNumber: idx + 1,
      raw: line,
      isComment: false,
      result: null,
      numericValue: null,
      error: false
    }
  })
})

const summaryStats = computed(() => {
  const nums = evaluatedLines.value
    .map((l) => l.numericValue)
    .filter((v): v is number => v !== null && !Number.isNaN(v))
  const sum = nums.reduce((a, b) => a + b, 0)
  const avg = nums.length > 0 ? sum / nums.length : 0
  return {
    count: nums.length,
    sum: Number(sum.toFixed(2)),
    avg: Number(avg.toFixed(2))
  }
})

async function createNewSheet() {
  const newSheet: CalcSheet = {
    id: `sheet-${Date.now()}`,
    title: `新建稿纸 #${sheets.value.length + 1}`,
    content: `# 在此输入算式或变量赋值\n单价 = 199\n数量 = 5\n总价 = 单价 * 数量`,
    updatedAt: Date.now()
  }
  sheets.value.unshift(newSheet)
  activeSheetId.value = newSheet.id
  await saveSheets()
}

async function deleteSheet(id: string) {
  if (sheets.value.length <= 1) {
    store.showToast('至少保留一份计算稿纸', 'warning')
    return
  }
  sheets.value = sheets.value.filter((s) => s.id !== id)
  if (activeSheetId.value === id) {
    activeSheetId.value = sheets.value[0].id
  }
  await saveSheets()
}

async function copyResult(val: string | number) {
  const str = String(val)
  await navigator.clipboard.writeText(str)
  store.showToast(`已复制计算结果：${str}`)
}

async function saveSheets() {
  await kvPut('calc-pad', 'sheets', sheets.value)
}

watch(
  sheets,
  () => {
    saveSheets()
  },
  { deep: true }
)

onMounted(async () => {
  sheets.value = await kvGet<CalcSheet[]>('calc-pad', 'sheets', DEFAULT_SHEETS)
  if (sheets.value.length > 0) {
    activeSheetId.value = sheets.value[0].id
  }
  if (store.pluginInitialPayload) {
    activeSheet.value.content += `\n${store.pluginInitialPayload}`
    store.pluginInitialPayload = ''
  }
})
</script>

<template>
  <div
    class="flex flex-1 h-full min-h-[500px] overflow-hidden select-none"
    :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
  >
    <!-- Left Sidebar: Sheets List -->
    <aside
      class="w-52 border-r flex flex-col shrink-0"
      :class="
        store.isDark
          ? 'bg-[#090C15] border-slate-800'
          : 'bg-[#F1F5F9] border-slate-200'
      "
    >
      <div class="flex items-center justify-between px-3 py-2.5 border-b border-inherit">
        <span class="text-xs font-semibold tracking-wide text-slate-400">
          稿纸工作区 ({{ sheets.length }})
        </span>
        <button
          type="button"
          title="新建计算稿纸"
          aria-label="新建计算稿纸"
          class="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium bg-indigo-600 text-white hover:bg-indigo-500 cursor-pointer transition-colors shadow-2xs"
          @click="createNewSheet"
        >
          <IconRenderer name="Plus" :size="13" />
          <span>新建</span>
        </button>
      </div>

      <div class="flex-1 overflow-y-auto p-2 space-y-1">
        <div
          v-for="sheet in sheets"
          :key="sheet.id"
          class="group flex items-center justify-between px-2.5 py-2 rounded-xl text-xs cursor-pointer transition-all"
          :class="
            sheet.id === activeSheetId
              ? store.isDark
                ? 'bg-indigo-500/15 text-indigo-300 font-semibold ring-1 ring-indigo-500/30'
                : 'bg-white text-indigo-600 font-semibold shadow-2xs ring-1 ring-slate-200/90'
              : store.isDark
                ? 'text-slate-400 hover:bg-slate-800/50 hover:text-slate-200'
                : 'text-slate-600 hover:bg-white/60'
          "
          @click="activeSheetId = sheet.id"
        >
          <div class="truncate pr-2">
            <div class="truncate">{{ sheet.title }}</div>
            <div class="text-[10px] text-slate-400 font-mono mt-0.5">
              {{ sheet.content.split('\n').length }} 行算式
            </div>
          </div>
          <button
            v-if="sheets.length > 1"
            type="button"
            aria-label="删除此稿纸"
            class="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-rose-500/15 hover:text-rose-500 transition-opacity cursor-pointer"
            @click.stop="deleteSheet(sheet.id)"
          >
            <IconRenderer name="Trash2" :size="13" />
          </button>
        </div>
      </div>
    </aside>

    <!-- Main Calc Pad Split Area -->
    <div v-if="activeSheet" class="flex-1 flex flex-col min-w-0">
      <!-- Sheet Title Toolbar -->
      <div
        class="flex items-center justify-between px-4 py-2 border-b"
        :class="store.isDark ? 'border-slate-800/80 bg-obsidian-900/40' : 'border-slate-200/80 bg-white'"
      >
        <input
          v-model="activeSheet.title"
          type="text"
          aria-label="稿纸标题"
          class="bg-transparent text-sm font-semibold outline-none focus:ring-2 focus:ring-indigo-500/30 rounded px-1.5 py-0.5"
          :class="store.isDark ? 'text-slate-100' : 'text-slate-800'"
        />
        <div class="flex items-center gap-2 text-[11px] text-slate-400">
          <span>
            支持中文变量赋值、单位转换 (
            <code
              class="font-mono px-1 py-0.5 rounded"
              :class="store.isDark ? 'bg-indigo-500/15 text-indigo-300' : 'bg-indigo-50 text-indigo-600'"
            >
              1024 MB to GB
            </code>
            )
          </span>
        </div>
      </div>

      <!-- Synchronized Editor & Live Result Column -->
      <div class="flex-1 flex overflow-y-auto font-mono text-[13px] leading-[28px]">
        <!-- Left: Line Numbers + Expression Textarea -->
        <div class="flex flex-1 min-w-0 relative">
          <!-- Line Numbers Gutter -->
          <div
            class="w-10 py-3 text-right pr-3 select-none text-[11px] shrink-0 border-r"
            :class="
              store.isDark
                ? 'bg-obsidian-950/30 border-slate-800/60 text-slate-600'
                : 'bg-slate-50/70 border-slate-200/70 text-slate-400'
            "
          >
            <div v-for="l in evaluatedLines" :key="l.lineNumber" class="h-[28px]">
              {{ l.lineNumber }}
            </div>
          </div>

          <!-- Expression Input -->
          <textarea
            v-model="activeSheet.content"
            spellcheck="false"
            aria-label="计算稿纸多行算式编辑区"
            placeholder="输入数学表达式或变量，例如：月薪 = 30000"
            class="flex-1 w-full p-3 bg-transparent resize-none outline-none leading-[28px] overflow-hidden"
            :class="store.isDark ? 'text-slate-100' : 'text-slate-800'"
            :style="{ minHeight: `${Math.max(420, evaluatedLines.length * 28 + 40)}px` }"
          ></textarea>
        </div>

        <!-- Right: Live Evaluated Results Column -->
        <div
          class="w-56 py-3 px-3 border-l shrink-0 select-text"
          :class="
            store.isDark
              ? 'bg-obsidian-950/40 border-slate-800/80'
              : 'bg-slate-50/90 border-slate-200/80'
          "
        >
          <div
            v-for="l in evaluatedLines"
            :key="l.lineNumber"
            class="h-[28px] flex items-center justify-end"
          >
            <button
              v-if="l.result !== null"
              type="button"
              title="点击复制此行计算结果"
              class="group flex items-center gap-1.5 px-2 py-0.5 rounded-md font-semibold cursor-pointer transition-colors"
              :class="
                store.isDark
                  ? 'text-indigo-300 hover:bg-indigo-500/15'
                  : 'text-indigo-600 hover:bg-indigo-50'
              "
              @click="copyResult(l.result)"
            >
              <span>= {{ l.result }}</span>
              <IconRenderer
                name="Copy"
                :size="12"
                class-name="opacity-0 group-hover:opacity-100 transition-opacity"
              />
            </button>
            <span v-else-if="l.isComment" class="text-[11px] text-slate-400 italic">
              注释
            </span>
          </div>
        </div>
      </div>

      <!-- Bottom Aggregation Bar -->
      <div
        class="flex items-center justify-between px-4 py-2 text-xs border-t font-mono"
        :class="
          store.isDark
            ? 'bg-obsidian-950/70 border-slate-800/80 text-slate-400'
            : 'bg-slate-100/80 border-slate-200/80 text-slate-600'
        "
      >
        <div class="flex items-center gap-4">
          <span>
            有效算式行:
            <strong :class="store.isDark ? 'text-slate-200' : 'text-slate-800'">{{ summaryStats.count }}</strong>
          </span>
          <button
            type="button"
            class="flex items-center gap-1 hover:text-indigo-500 cursor-pointer transition-colors"
            @click="copyResult(summaryStats.sum)"
          >
            <span>SUM 总和:</span>
            <strong :class="store.isDark ? 'text-indigo-300' : 'text-indigo-600'">
              {{ summaryStats.sum.toLocaleString() }}
            </strong>
            <IconRenderer name="Copy" :size="12" />
          </button>
          <button
            type="button"
            class="flex items-center gap-1 hover:text-indigo-500 cursor-pointer transition-colors"
            @click="copyResult(summaryStats.avg)"
          >
            <span>AVG 均值:</span>
            <strong :class="store.isDark ? 'text-violet-300' : 'text-violet-600'">
              {{ summaryStats.avg.toLocaleString() }}
            </strong>
          </button>
        </div>
        <span class="text-[11px] text-slate-400">点击右侧任意结果立即复制</span>
      </div>
    </div>
  </div>
</template>
