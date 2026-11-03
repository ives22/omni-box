<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import {
  PortListenEntry,
  fetchListeningPorts,
  killPortProcess,
  openUrlInSystemBrowser,
  revealPathInFinder
} from '@/bridge/backend'
import IconRenderer from '@/components/IconRenderer.vue'

const store = useLauncherStore()

const portsList = ref<PortListenEntry[]>([])
const loading = ref(false)
const scanDurationMs = ref<number>(0)
const activeCategory = ref<'all' | 'web' | 'database' | 'app' | 'system'>('all')
const onlyPublicExposed = ref(false)
const includeUdp = ref(false)
const autoRefresh = ref(false)
const sortBy = ref<'smart' | 'port' | 'memory' | 'cpu'>('smart')
const selectedEntryId = ref<string | null>(null)

// Kill confirmation & admin escalation state
const pendingKillConfirm = ref<{
  entry: PortListenEntry
  force: boolean
  needAdmin: boolean
} | null>(null)
const isKilling = ref(false)

let refreshTimer: ReturnType<typeof setInterval> | null = null

const categoryCounts = computed(() => {
  const counts = {
    all: portsList.value.length,
    web: 0,
    database: 0,
    app: 0,
    system: 0,
    exposed: 0
  }
  for (const item of portsList.value) {
    if (item.category === 'web') counts.web++
    else if (item.category === 'database') counts.database++
    else if (item.category === 'app') counts.app++
    else counts.system++
    if (item.isPublicExposed) counts.exposed++
  }
  return counts
})

const filteredPorts = computed(() => {
  const rawQuery = store.subInputQuery.trim().toLowerCase().replace(/^:/, '')
  let list = portsList.value.filter((item) => {
    if (activeCategory.value !== 'all' && item.category !== activeCategory.value) {
      return false
    }
    if (onlyPublicExposed.value && !item.isPublicExposed) {
      return false
    }
    if (!rawQuery) return true
    const portStr = String(item.port)
    const pidStr = String(item.pid)
    return (
      portStr.includes(rawQuery) ||
      pidStr.includes(rawQuery) ||
      item.displayName.toLowerCase().includes(rawQuery) ||
      item.processName.toLowerCase().includes(rawQuery) ||
      item.commandArgs.toLowerCase().includes(rawQuery) ||
      item.cwd.toLowerCase().includes(rawQuery) ||
      item.user.toLowerCase().includes(rawQuery) ||
      item.bindAddr.toLowerCase().includes(rawQuery)
    )
  })

  if (sortBy.value === 'port') {
    list = [...list].sort((a, b) => a.port - b.port)
  } else if (sortBy.value === 'memory') {
    list = [...list].sort((a, b) => b.memoryMb - a.memoryMb || a.port - b.port)
  } else if (sortBy.value === 'cpu') {
    list = [...list].sort((a, b) => b.cpuPercent - a.cpuPercent || a.port - b.port)
  }
  return list
})

const selectedEntry = computed<PortListenEntry | null>(() => {
  if (!filteredPorts.value.length) return null
  if (selectedEntryId.value) {
    const found = filteredPorts.value.find((i) => i.id === selectedEntryId.value)
    if (found) return found
  }
  return filteredPorts.value[0] || null
})

watch(filteredPorts, (nextList) => {
  if (!nextList.length) {
    selectedEntryId.value = null
    return
  }
  if (!selectedEntryId.value || !nextList.some((i) => i.id === selectedEntryId.value)) {
    selectedEntryId.value = nextList[0]!.id
  }
})

async function loadListeningPorts(silent = false) {
  if (!silent) {
    loading.value = true
  }
  const t0 = performance.now()
  try {
    const data = await fetchListeningPorts(includeUdp.value)
    portsList.value = data
    scanDurationMs.value = Math.max(1, Math.round(performance.now() - t0))
  } catch (e) {
    if (!silent) {
      store.showToast(`扫描端口失败：${String(e)}`, 'warning')
    }
  } finally {
    loading.value = false
  }
}

watch(includeUdp, () => {
  loadListeningPorts(false)
})

watch(autoRefresh, (enabled) => {
  if (refreshTimer) {
    clearInterval(refreshTimer)
    refreshTimer = null
  }
  if (enabled) {
    refreshTimer = setInterval(() => {
      loadListeningPorts(true)
    }, 3000)
  }
})

function categoryBadgeMeta(cat: string) {
  switch (cat) {
    case 'web':
      return {
        label: 'Web / 开发',
        portCls: store.isDark
          ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
          : 'bg-indigo-50 text-indigo-700 border-indigo-200',
        tagCls: store.isDark
          ? 'bg-indigo-500/15 text-indigo-300'
          : 'bg-indigo-50 text-indigo-700'
      }
    case 'database':
      return {
        label: '数据库 / 容器',
        portCls: store.isDark
          ? 'bg-amber-500/15 text-amber-300 border-amber-500/30'
          : 'bg-amber-50 text-amber-800 border-amber-200',
        tagCls: store.isDark
          ? 'bg-amber-500/15 text-amber-300'
          : 'bg-amber-50 text-amber-800'
      }
    case 'app':
      return {
        label: '桌面应用',
        portCls: store.isDark
          ? 'bg-sky-500/15 text-sky-300 border-sky-500/30'
          : 'bg-sky-50 text-sky-700 border-sky-200',
        tagCls: store.isDark ? 'bg-sky-500/15 text-sky-300' : 'bg-sky-50 text-sky-700'
      }
    default:
      return {
        label: '系统服务',
        portCls: store.isDark
          ? 'bg-slate-800 text-slate-300 border-slate-700'
          : 'bg-slate-100 text-slate-700 border-slate-200',
        tagCls: store.isDark ? 'bg-slate-800 text-slate-400' : 'bg-slate-100 text-slate-600'
      }
  }
}

function formatMemory(mb: number): string {
  if (!mb || mb <= 0) return '-'
  if (mb >= 1024) {
    return `${(mb / 1024).toFixed(2)} GB`
  }
  return `${mb.toFixed(1)} MB`
}

async function copyText(text: string, label: string) {
  try {
    await navigator.clipboard.writeText(text)
    store.showToast(`已复制${label}：${text.slice(0, 48)}`)
  } catch {
    // ignore
  }
}

async function openPortInBrowser(entry: PortListenEntry) {
  const protocol = entry.port === 443 || entry.port === 8443 ? 'https' : 'http'
  const url = `${protocol}://127.0.0.1:${entry.port}`
  try {
    await openUrlInSystemBrowser(url)
    store.showToast(`已在浏览器打开 ${url}`)
  } catch (e) {
    store.showToast(`打开浏览器失败：${String(e)}`, 'warning')
  }
}

async function handleRevealPath(pathStr: string) {
  if (!pathStr || pathStr === '/') return
  try {
    await revealPathInFinder(pathStr)
  } catch (e) {
    store.showToast(`无法在访达中定位路径：${String(e)}`, 'warning')
  }
}

function selectSiblingPort(portNum: number, pid: number) {
  const match = portsList.value.find((i) => i.port === portNum && i.pid === pid)
  if (match) {
    selectedEntryId.value = match.id
  } else {
    store.subInputQuery = String(portNum)
  }
}

function requestKillProcess(entry: PortListenEntry, force = false) {
  if (entry.pid <= 1 || entry.displayName.includes('当前应用')) {
    store.showToast('受保护进程禁止终止', 'warning')
    return
  }
  pendingKillConfirm.value = {
    entry,
    force,
    needAdmin: entry.user === 'root'
  }
}

async function confirmAndExecuteKill(useAdminOverride?: boolean) {
  const ctx = pendingKillConfirm.value
  if (!ctx || isKilling.value) return

  const useAdmin = useAdminOverride ?? ctx.needAdmin
  isKilling.value = true
  try {
    const msg = await killPortProcess(ctx.entry.pid, ctx.force, useAdmin)
    store.showToast(msg)
    pendingKillConfirm.value = null
    await loadListeningPorts(false)
  } catch (e) {
    const errStr = String((e as Error)?.message || e || '')
    if (errStr.includes('PERMISSION_DENIED_NEED_ADMIN')) {
      pendingKillConfirm.value = {
        ...ctx,
        needAdmin: true
      }
      store.showToast('普通权限不足，请点击「使用管理员权限强制终止」', 'warning')
      return
    }
    store.showToast(errStr, 'warning')
  } finally {
    isKilling.value = false
  }
}

function handleKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'r') {
    e.preventDefault()
    loadListeningPorts(false)
    store.showToast('已刷新本机端口监听列表')
    return
  }

  if (pendingKillConfirm.value) {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      pendingKillConfirm.value = null
    }
    return
  }

  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    const list = filteredPorts.value
    if (!list.length) return
    e.preventDefault()
    const curIdx = list.findIndex((i) => i.id === selectedEntry.value?.id)
    const nextIdx =
      e.key === 'ArrowDown'
        ? (curIdx + 1) % list.length
        : (curIdx - 1 + list.length) % list.length
    selectedEntryId.value = list[nextIdx]!.id
  }
}

onMounted(async () => {
  const winObj = window as unknown as {
    __omniboxPluginInterceptEscape?: () => boolean
  }
  winObj.__omniboxPluginInterceptEscape = () => {
    if (pendingKillConfirm.value) {
      pendingKillConfirm.value = null
      return true
    }
    return false
  }

  // Handle initial payload from main search bar (e.g. `:3000` or `kill:3000:48210`)
  const initPayload = (store.pluginInitialPayload || '').trim()
  if (initPayload) {
    store.pluginInitialPayload = ''
    if (initPayload.startsWith('port:')) {
      const portPart = initPayload.replace(/^port:/, '').trim()
      store.subInputQuery = portPart
    } else {
      store.subInputQuery = initPayload.replace(/^:/, '')
    }
  }

  await loadListeningPorts(false)
  window.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  if (refreshTimer) {
    clearInterval(refreshTimer)
    refreshTimer = null
  }
  const winObj = window as unknown as {
    __omniboxPluginInterceptEscape?: () => boolean
  }
  delete winObj.__omniboxPluginInterceptEscape
  window.removeEventListener('keydown', handleKeydown)
})
</script>

<template>
  <div class="flex flex-col h-full min-h-0 select-none relative">
    <!-- Top Filter & Telemetry Control Bar -->
    <div
      class="px-4 py-2.5 border-b flex items-center justify-between gap-2 shrink-0"
      :class="
        store.isDark
          ? 'bg-[#0B0F19]/90 border-slate-800/90'
          : 'bg-white border-slate-200/90'
      "
    >
      <!-- Category Pills -->
      <div class="flex items-center gap-1 overflow-x-auto no-scrollbar">
        <button
          v-for="tab in [
            { id: 'all', label: '全部监听', count: categoryCounts.all },
            { id: 'web', label: 'Web 与开发', count: categoryCounts.web },
            { id: 'database', label: '数据库与容器', count: categoryCounts.database },
            { id: 'app', label: '桌面应用', count: categoryCounts.app },
            { id: 'system', label: '系统服务', count: categoryCounts.system }
          ]"
          :key="tab.id"
          type="button"
          class="px-2.5 py-1 rounded-lg text-[11.5px] font-medium transition-all cursor-pointer flex items-center gap-1.5 shrink-0"
          :class="
            activeCategory === tab.id
              ? store.isDark
                ? 'bg-indigo-500/20 text-indigo-300 border border-indigo-500/35 shadow-xs'
                : 'bg-indigo-600 text-white shadow-xs'
              : store.isDark
                ? 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/70'
                : 'text-slate-600 hover:text-slate-900 hover:bg-slate-100'
          "
          @click="activeCategory = tab.id as any"
        >
          <span>{{ tab.label }}</span>
          <span
            class="px-1.5 py-0.2 rounded-full text-[10px] font-mono"
            :class="
              activeCategory === tab.id
                ? store.isDark
                  ? 'bg-indigo-500/30 text-indigo-200'
                  : 'bg-white/20 text-white'
                : store.isDark
                  ? 'bg-slate-800 text-slate-400'
                  : 'bg-slate-200/80 text-slate-600'
            "
          >
            {{ tab.count }}
          </span>
        </button>
      </div>

      <!-- Right Controls: Public Filter, UDP Toggle, Auto Refresh, Manual Refresh -->
      <div class="flex items-center gap-1.5 shrink-0">
        <!-- Public Exposed Filter Toggle -->
        <button
          type="button"
          title="仅显示绑定在 0.0.0.0 / * 上允许局域网外部设备访问的端口"
          class="px-2 py-1 rounded-lg text-[11px] font-medium border transition-colors cursor-pointer flex items-center gap-1"
          :class="
            onlyPublicExposed
              ? store.isDark
                ? 'bg-amber-500/20 border-amber-500/40 text-amber-300'
                : 'bg-amber-50 border-amber-300 text-amber-800'
              : store.isDark
                ? 'bg-slate-900 border-slate-800 text-slate-400 hover:text-slate-200'
                : 'bg-slate-50 border-slate-200/90 text-slate-600 hover:text-slate-900'
          "
          @click="onlyPublicExposed = !onlyPublicExposed"
        >
          <span
            class="w-1.5 h-1.5 rounded-full"
            :class="onlyPublicExposed ? 'bg-amber-400' : 'bg-slate-400'"
          ></span>
          <span>外部暴露 ({{ categoryCounts.exposed }})</span>
        </button>

        <!-- UDP Toggle -->
        <button
          type="button"
          title="同时扫描本机绑定的 UDP 端口"
          class="px-2 py-1 rounded-lg text-[11px] font-medium border transition-colors cursor-pointer"
          :class="
            includeUdp
              ? store.isDark
                ? 'bg-teal-500/20 border-teal-500/40 text-teal-300'
                : 'bg-teal-50 border-teal-300 text-teal-800'
              : store.isDark
                ? 'bg-slate-900 border-slate-800 text-slate-400 hover:text-slate-200'
                : 'bg-slate-50 border-slate-200/90 text-slate-600 hover:text-slate-900'
          "
          @click="includeUdp = !includeUdp"
        >
          +UDP
        </button>

        <!-- Auto Refresh 3s Toggle -->
        <button
          type="button"
          title="开启后每 3 秒自动静默刷新端口与进程状态"
          class="px-2 py-1 rounded-lg text-[11px] font-medium border transition-colors cursor-pointer flex items-center gap-1"
          :class="
            autoRefresh
              ? store.isDark
                ? 'bg-emerald-500/20 border-emerald-500/40 text-emerald-300'
                : 'bg-emerald-50 border-emerald-300 text-emerald-700'
              : store.isDark
                ? 'bg-slate-900 border-slate-800 text-slate-400 hover:text-slate-200'
                : 'bg-slate-50 border-slate-200/90 text-slate-600 hover:text-slate-900'
          "
          @click="autoRefresh = !autoRefresh"
        >
          <span
            class="w-1.5 h-1.5 rounded-full"
            :class="autoRefresh ? 'bg-emerald-400 animate-ping' : 'bg-slate-400'"
          ></span>
          <span>实时 3s</span>
        </button>

        <!-- Manual Refresh Button -->
        <button
          type="button"
          title="立即重新扫描本机监听端口 (⌘R)"
          class="px-2.5 py-1 rounded-lg text-[11px] font-medium border transition-colors cursor-pointer flex items-center gap-1"
          :class="
            store.isDark
              ? 'bg-slate-900 hover:bg-slate-800 border-slate-800 text-slate-200'
              : 'bg-white hover:bg-slate-50 border-slate-200 text-slate-700 shadow-2xs'
          "
          @click="loadListeningPorts(false)"
        >
          <IconRenderer
            name="RefreshCw"
            :size="12"
            :class-name="loading ? 'animate-spin text-indigo-400' : ''"
          />
          <span>刷新</span>
          <span
            v-if="scanDurationMs"
            class="text-[10px] font-mono text-slate-400 ml-0.5"
          >
            {{ scanDurationMs }}ms
          </span>
        </button>
      </div>
    </div>

    <!-- Main Master-Detail Split View -->
    <div class="flex-1 min-h-0 flex overflow-hidden">
      <!-- Left Column: Listening Ports Table / List (56% width) -->
      <div
        class="w-[56%] flex flex-col min-h-0 border-r"
        :class="store.isDark ? 'border-slate-800/90' : 'border-slate-200/90'"
      >
        <!-- Sub-header Sort Bar -->
        <div
          class="px-3.5 py-1.5 border-b text-[11px] flex items-center justify-between"
          :class="
            store.isDark
              ? 'bg-slate-900/50 border-slate-800/80 text-slate-400'
              : 'bg-slate-50 border-slate-200/70 text-slate-500'
          "
        >
          <span>共匹配 {{ filteredPorts.length }} 个监听端点</span>
          <div class="flex items-center gap-1">
            <span>排序：</span>
            <button
              v-for="s in [
                { id: 'smart', label: '开发优先' },
                { id: 'port', label: '端口号' },
                { id: 'memory', label: '内存占用' },
                { id: 'cpu', label: 'CPU' }
              ]"
              :key="s.id"
              type="button"
              class="px-1.5 py-0.5 rounded cursor-pointer transition-colors"
              :class="
                sortBy === s.id
                  ? store.isDark
                    ? 'bg-indigo-500/20 text-indigo-300 font-semibold'
                    : 'bg-indigo-100 text-indigo-700 font-semibold'
                  : 'hover:underline'
              "
              @click="sortBy = s.id as any"
            >
              {{ s.label }}
            </button>
          </div>
        </div>

        <!-- Scrollable Port Rows -->
        <div class="flex-1 overflow-y-auto divide-y" :class="store.isDark ? 'divide-slate-800/60' : 'divide-slate-200/60'">
          <div
            v-if="filteredPorts.length === 0 && !loading"
            class="h-full flex flex-col items-center justify-center p-8 text-center text-slate-400 space-y-2"
          >
            <IconRenderer name="Server" :size="28" class-name="opacity-40" />
            <p class="text-xs font-medium">未找到匹配的监听端口</p>
            <p class="text-[11px] text-slate-500">
              可尝试切换上方分类标签，或在顶部搜索框输入端口号（如 3000 / 8080）或进程名（如 node）
            </p>
          </div>

          <div
            v-for="item in filteredPorts"
            :key="item.id"
            class="group px-3.5 py-2.5 flex items-center justify-between gap-2.5 cursor-pointer transition-colors"
            :class="
              selectedEntry?.id === item.id
                ? store.isDark
                  ? 'bg-indigo-500/12'
                  : 'bg-indigo-50/90'
                : store.isDark
                  ? 'hover:bg-slate-900/70'
                  : 'hover:bg-slate-100/70'
            "
            @click="selectedEntryId = item.id"
          >
            <!-- Left: Port Badge + Bind Scope -->
            <div class="flex items-center gap-2.5 min-w-0 flex-1">
              <div
                class="w-[74px] py-1 px-2 rounded-lg border font-mono text-xs font-bold flex items-center justify-between shrink-0"
                :class="categoryBadgeMeta(item.category).portCls"
              >
                <span>:{{ item.port }}</span>
                <span class="text-[9.5px] font-normal opacity-75">{{ item.protocol }}</span>
              </div>

              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-1.5">
                  <span
                    class="text-xs font-semibold truncate"
                    :class="store.isDark ? 'text-slate-100' : 'text-slate-800'"
                  >
                    {{ item.displayName }}
                  </span>
                  <span
                    v-if="item.isSystemProtected"
                    title="系统核心服务或当前应用（防误杀保护）"
                    class="text-slate-400 shrink-0"
                  >
                    <IconRenderer name="Lock" :size="11" />
                  </span>
                </div>

                <div class="flex items-center gap-2 mt-0.5 text-[10.5px] text-slate-400 font-mono">
                  <span>PID {{ item.pid }}</span>
                  <span>·</span>
                  <span
                    class="inline-flex items-center gap-1"
                    :class="
                      item.isPublicExposed
                        ? 'text-amber-500 dark:text-amber-400 font-medium'
                        : 'text-emerald-600 dark:text-emerald-400'
                    "
                  >
                    <span
                      class="w-1.5 h-1.5 rounded-full"
                      :class="item.isPublicExposed ? 'bg-amber-400' : 'bg-emerald-400'"
                    ></span>
                    {{ item.isPublicExposed ? `${item.bindAddr} (暴露)` : '127.0.0.1 (本机)' }}
                  </span>
                  <span v-if="item.memoryMb > 0">· {{ formatMemory(item.memoryMb) }}</span>
                </div>
              </div>
            </div>

            <!-- Right: Quick Hover Actions -->
            <div class="flex items-center gap-1 shrink-0">
              <button
                v-if="item.protocol === 'TCP' && item.category !== 'system'"
                type="button"
                title="在浏览器中打开 http://127.0.0.1:端口"
                class="p-1.5 rounded-lg opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer"
                :class="
                  store.isDark
                    ? 'text-slate-400 hover:text-indigo-300 hover:bg-slate-800'
                    : 'text-slate-500 hover:text-indigo-600 hover:bg-slate-200/70'
                "
                @click.stop="openPortInBrowser(item)"
              >
                <IconRenderer name="ExternalLink" :size="13" />
              </button>

              <button
                type="button"
                title="复制 127.0.0.1:端口 地址"
                class="p-1.5 rounded-lg opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer"
                :class="
                  store.isDark
                    ? 'text-slate-400 hover:text-slate-200 hover:bg-slate-800'
                    : 'text-slate-500 hover:text-slate-800 hover:bg-slate-200/70'
                "
                @click.stop="copyText(`127.0.0.1:${item.port}`, '端口地址')"
              >
                <IconRenderer name="Copy" :size="13" />
              </button>

              <button
                v-if="!item.isSystemProtected"
                type="button"
                title="终止该进程并释放端口"
                class="px-2 py-1 rounded-lg text-[11px] font-medium border transition-colors cursor-pointer flex items-center gap-1"
                :class="
                  store.isDark
                    ? 'bg-rose-500/10 hover:bg-rose-500/25 border-rose-500/30 text-rose-300'
                    : 'bg-rose-50 hover:bg-rose-100 border-rose-200 text-rose-700'
                "
                @click.stop="requestKillProcess(item, false)"
              >
                <IconRenderer name="OctagonX" :size="12" />
                <span>结束</span>
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Right Column: Process Deep Inspector & Control Panel (44% width) -->
      <div
        class="w-[44%] flex flex-col min-h-0 overflow-y-auto p-4 space-y-3.5"
        :class="store.isDark ? 'bg-[#0B0F19]/50' : 'bg-slate-50/70'"
      >
        <template v-if="selectedEntry">
          <!-- 1. Header Identity Card -->
          <div
            class="p-3.5 rounded-2xl border space-y-3"
            :class="
              store.isDark
                ? 'bg-slate-900/90 border-slate-800'
                : 'bg-white border-slate-200/90 shadow-2xs'
            "
          >
            <div class="flex items-start justify-between gap-2">
              <div>
                <div class="flex items-center gap-2">
                  <span
                    class="text-lg font-extrabold font-mono tracking-tight"
                    :class="store.isDark ? 'text-white' : 'text-slate-900'"
                  >
                    :{{ selectedEntry.port }}
                  </span>
                  <span
                    class="px-2 py-0.5 rounded-md text-[10.5px] font-semibold"
                    :class="categoryBadgeMeta(selectedEntry.category).tagCls"
                  >
                    {{ categoryBadgeMeta(selectedEntry.category).label }}
                  </span>
                  <span
                    class="px-2 py-0.5 rounded-md text-[10.5px] font-mono"
                    :class="
                      store.isDark
                        ? 'bg-slate-800 text-slate-300'
                        : 'bg-slate-100 text-slate-700'
                    "
                  >
                    {{ selectedEntry.protocol }} · {{ selectedEntry.ipVersion }}
                  </span>
                </div>

                <h3
                  class="text-sm font-bold mt-1 break-all"
                  :class="store.isDark ? 'text-slate-100' : 'text-slate-800'"
                >
                  {{ selectedEntry.displayName }}
                </h3>
              </div>

              <!-- Exposure Badge -->
              <span
                class="px-2 py-1 rounded-lg text-[10.5px] font-medium border shrink-0 flex items-center gap-1"
                :class="
                  selectedEntry.isPublicExposed
                    ? store.isDark
                      ? 'bg-amber-500/15 border-amber-500/35 text-amber-300'
                      : 'bg-amber-50 border-amber-200 text-amber-800'
                    : store.isDark
                      ? 'bg-emerald-500/15 border-emerald-500/35 text-emerald-300'
                      : 'bg-emerald-50 border-emerald-200 text-emerald-700'
                "
              >
                <span
                  class="w-1.5 h-1.5 rounded-full"
                  :class="selectedEntry.isPublicExposed ? 'bg-amber-400' : 'bg-emerald-400'"
                ></span>
                {{ selectedEntry.isPublicExposed ? '局域网暴露 (0.0.0.0)' : '仅本机回环' }}
              </span>
            </div>

            <!-- Quick Utility Buttons -->
            <div class="flex flex-wrap items-center gap-1.5 pt-1">
              <button
                v-if="selectedEntry.protocol === 'TCP'"
                type="button"
                class="px-2.5 py-1 rounded-lg text-[11px] font-medium border cursor-pointer transition-colors flex items-center gap-1"
                :class="
                  store.isDark
                    ? 'bg-indigo-500/15 hover:bg-indigo-500/25 border-indigo-500/30 text-indigo-300'
                    : 'bg-indigo-50 hover:bg-indigo-100 border-indigo-200 text-indigo-700'
                "
                @click="openPortInBrowser(selectedEntry)"
              >
                <IconRenderer name="ExternalLink" :size="12" />
                <span>浏览器打开</span>
              </button>

              <button
                type="button"
                class="px-2.5 py-1 rounded-lg text-[11px] font-medium border cursor-pointer transition-colors flex items-center gap-1"
                :class="
                  store.isDark
                    ? 'bg-slate-800 hover:bg-slate-700 border-slate-700 text-slate-200'
                    : 'bg-slate-100 hover:bg-slate-200 border-slate-200 text-slate-700'
                "
                @click="copyText(`http://127.0.0.1:${selectedEntry.port}`, '本地 URL')"
              >
                <IconRenderer name="Copy" :size="12" />
                <span>复制 URL</span>
              </button>

              <button
                type="button"
                class="px-2.5 py-1 rounded-lg text-[11px] font-medium border cursor-pointer transition-colors flex items-center gap-1"
                :class="
                  store.isDark
                    ? 'bg-slate-800 hover:bg-slate-700 border-slate-700 text-slate-200'
                    : 'bg-slate-100 hover:bg-slate-200 border-slate-200 text-slate-700'
                "
                @click="copyText(String(selectedEntry.pid), '进程 PID')"
              >
                <span>PID: {{ selectedEntry.pid }}</span>
              </button>
            </div>
          </div>

          <!-- 2. Telemetry 4-Grid -->
          <div class="grid grid-cols-2 gap-2 text-xs">
            <div
              class="p-2.5 rounded-xl border"
              :class="
                store.isDark
                  ? 'bg-slate-900/75 border-slate-800'
                  : 'bg-white border-slate-200/80'
              "
            >
              <div class="text-[10.5px] text-slate-400">进程 PID / 父进程 PPID</div>
              <div class="font-mono font-bold mt-0.5">
                {{ selectedEntry.pid }}
                <span class="text-slate-400 font-normal">/ {{ selectedEntry.ppid }}</span>
              </div>
            </div>

            <div
              class="p-2.5 rounded-xl border"
              :class="
                store.isDark
                  ? 'bg-slate-900/75 border-slate-800'
                  : 'bg-white border-slate-200/80'
              "
            >
              <div class="text-[10.5px] text-slate-400">物理内存 / CPU 占用</div>
              <div class="font-mono font-bold mt-0.5">
                {{ formatMemory(selectedEntry.memoryMb) }}
                <span class="text-slate-400 font-normal">· {{ selectedEntry.cpuPercent }}%</span>
              </div>
            </div>

            <div
              class="p-2.5 rounded-xl border"
              :class="
                store.isDark
                  ? 'bg-slate-900/75 border-slate-800'
                  : 'bg-white border-slate-200/80'
              "
            >
              <div class="text-[10.5px] text-slate-400">所属系统用户</div>
              <div class="font-mono font-bold mt-0.5 flex items-center gap-1.5">
                <span>{{ selectedEntry.user }}</span>
                <span
                  v-if="selectedEntry.user === 'root'"
                  class="px-1.5 py-0.2 text-[9.5px] rounded bg-amber-500/20 text-amber-400"
                >
                  管理员进程
                </span>
              </div>
            </div>

            <div
              class="p-2.5 rounded-xl border"
              :class="
                store.isDark
                  ? 'bg-slate-900/75 border-slate-800'
                  : 'bg-white border-slate-200/80'
              "
            >
              <div class="text-[10.5px] text-slate-400">已持续运行时间</div>
              <div class="font-mono font-bold mt-0.5">{{ selectedEntry.elapsedTime }}</div>
            </div>
          </div>

          <!-- 3. Sibling Ports listened by the same PID -->
          <div
            v-if="selectedEntry.siblingPorts.length > 1"
            class="p-3 rounded-xl border space-y-1.5"
            :class="
              store.isDark
                ? 'bg-slate-900/75 border-slate-800'
                : 'bg-white border-slate-200/80'
            "
          >
            <div class="text-[11px] text-slate-400 flex items-center justify-between">
              <span>该进程 (PID {{ selectedEntry.pid }}) 同时监听的 {{ selectedEntry.siblingPorts.length }} 个端口：</span>
            </div>
            <div class="flex flex-wrap gap-1.5">
              <button
                v-for="sp in selectedEntry.siblingPorts"
                :key="sp"
                type="button"
                class="px-2 py-0.5 rounded-md font-mono text-[11px] border cursor-pointer transition-colors"
                :class="
                  sp === selectedEntry.port
                    ? 'bg-indigo-600 text-white border-indigo-500 font-bold'
                    : store.isDark
                      ? 'bg-slate-800 border-slate-700 text-slate-300 hover:border-indigo-400'
                      : 'bg-slate-100 border-slate-200 text-slate-700 hover:border-indigo-400'
                "
                @click="selectSiblingPort(sp, selectedEntry.pid)"
              >
                :{{ sp }}
              </button>
            </div>
          </div>

          <!-- 4. Working Directory & Executable Path -->
          <div
            class="p-3 rounded-xl border space-y-2.5 text-xs"
            :class="
              store.isDark
                ? 'bg-slate-900/75 border-slate-800'
                : 'bg-white border-slate-200/80'
            "
          >
            <div v-if="selectedEntry.cwd && selectedEntry.cwd !== '/'">
              <div class="flex items-center justify-between text-[11px] text-slate-400 mb-1">
                <span>项目工作目录 (Working Directory)</span>
                <div class="flex items-center gap-2">
                  <button
                    type="button"
                    class="text-indigo-400 hover:underline cursor-pointer flex items-center gap-0.5"
                    @click="handleRevealPath(selectedEntry.cwd)"
                  >
                    <IconRenderer name="FolderOpen" :size="11" />
                    <span>在 Finder 打开</span>
                  </button>
                  <button
                    type="button"
                    class="text-slate-400 hover:underline cursor-pointer"
                    @click="copyText(selectedEntry.cwd, '工作目录')"
                  >
                    复制
                  </button>
                </div>
              </div>
              <div
                class="p-2 rounded-lg font-mono text-[11px] break-all select-text"
                :class="store.isDark ? 'bg-slate-950 text-emerald-300' : 'bg-slate-100 text-emerald-800'"
              >
                {{ selectedEntry.cwd }}
              </div>
            </div>

            <div>
              <div class="flex items-center justify-between text-[11px] text-slate-400 mb-1">
                <span>完整启动命令行 (Command Args)</span>
                <button
                  type="button"
                  class="text-indigo-400 hover:underline cursor-pointer"
                  @click="copyText(selectedEntry.commandArgs, '启动命令')"
                >
                  复制命令
                </button>
              </div>
              <div
                class="p-2 rounded-lg font-mono text-[11px] break-all max-h-24 overflow-y-auto select-text leading-relaxed"
                :class="store.isDark ? 'bg-slate-950 text-slate-300' : 'bg-slate-100 text-slate-700'"
              >
                {{ selectedEntry.commandArgs }}
              </div>
            </div>
          </div>

          <!-- 5. Process Termination Action Bar -->
          <div
            class="p-3.5 rounded-2xl border space-y-2.5"
            :class="
              selectedEntry.isSystemProtected
                ? store.isDark
                  ? 'bg-slate-900/50 border-slate-800'
                  : 'bg-slate-100/70 border-slate-200'
                : store.isDark
                  ? 'bg-rose-950/20 border-rose-500/25'
                  : 'bg-rose-50/60 border-rose-200/80'
            "
          >
            <div class="flex items-center justify-between">
              <span
                class="text-xs font-bold flex items-center gap-1.5"
                :class="
                  selectedEntry.isSystemProtected
                    ? 'text-slate-400'
                    : store.isDark
                      ? 'text-rose-200'
                      : 'text-rose-800'
                "
              >
                <IconRenderer
                  :name="selectedEntry.isSystemProtected ? 'Lock' : 'OctagonX'"
                  :size="14"
                />
                <span>
                  {{
                    selectedEntry.isSystemProtected
                      ? '系统服务与当前应用受防误杀保护'
                      : `关闭进程并释放 :${selectedEntry.port} 端口`
                  }}
                </span>
              </span>
            </div>

            <div v-if="!selectedEntry.isSystemProtected" class="grid grid-cols-2 gap-2 text-xs">
              <button
                type="button"
                class="py-2 px-3 rounded-xl font-semibold border cursor-pointer transition-colors flex items-center justify-center gap-1.5"
                :class="
                  store.isDark
                    ? 'bg-amber-500/15 hover:bg-amber-500/25 border-amber-500/35 text-amber-300'
                    : 'bg-amber-50 hover:bg-amber-100 border-amber-300 text-amber-800'
                "
                @click="requestKillProcess(selectedEntry, false)"
              >
                <IconRenderer name="Power" :size="13" />
                <span>优雅停止 (SIGTERM)</span>
              </button>

              <button
                type="button"
                class="py-2 px-3 rounded-xl font-semibold text-white bg-rose-600 hover:bg-rose-500 active:bg-rose-700 cursor-pointer transition-colors flex items-center justify-center gap-1.5 shadow-xs"
                @click="requestKillProcess(selectedEntry, true)"
              >
                <IconRenderer name="Zap" :size="13" />
                <span>强制秒杀 (kill -9)</span>
              </button>
            </div>
            <p v-else class="text-[11px] text-slate-400 leading-relaxed">
              该进程属于 macOS 系统底层守护服务或 OmniBox 自身进程，为防止系统异常已默认锁定。
            </p>
          </div>
        </template>
      </div>
    </div>

    <!-- Kill Confirmation & Admin Escalation Modal -->
    <div
      v-if="pendingKillConfirm"
      class="absolute inset-0 z-50 flex items-center justify-center bg-slate-950/60 backdrop-blur-xs p-4"
      @click.self="pendingKillConfirm = null"
    >
      <div
        class="w-full max-w-[420px] rounded-2xl border p-5 shadow-2xl space-y-4"
        :class="
          store.isDark
            ? 'bg-[#111827] border-slate-800 text-slate-100'
            : 'bg-white border-slate-200 text-slate-800'
        "
      >
        <div class="flex items-start justify-between">
          <div class="flex items-center gap-2.5">
            <div
              class="w-9 h-9 rounded-xl flex items-center justify-center shrink-0"
              :class="
                pendingKillConfirm.force
                  ? 'bg-rose-500/15 text-rose-500'
                  : 'bg-amber-500/15 text-amber-500'
              "
            >
              <IconRenderer :name="pendingKillConfirm.force ? 'Zap' : 'AlertTriangle'" :size="18" />
            </div>
            <div>
              <h3 class="text-sm font-bold">
                {{
                  pendingKillConfirm.force
                    ? '确认强制终止进程 (SIGKILL -9)'
                    : '确认停止进程 (SIGTERM -15)'
                }}
              </h3>
              <p class="text-[11px] text-slate-400 mt-0.5">
                PID {{ pendingKillConfirm.entry.pid }} · {{ pendingKillConfirm.entry.displayName }}
              </p>
            </div>
          </div>
          <button
            type="button"
            class="p-1 rounded-lg text-slate-400 hover:text-slate-200 cursor-pointer"
            @click="pendingKillConfirm = null"
          >
            <IconRenderer name="X" :size="15" />
          </button>
        </div>

        <div
          class="p-3 rounded-xl border text-xs space-y-1.5"
          :class="
            store.isDark
              ? 'bg-slate-900/90 border-slate-800 text-slate-300'
              : 'bg-slate-50 border-slate-200/80 text-slate-700'
          "
        >
          <div class="flex items-center justify-between">
            <span class="text-slate-400">释放端口：</span>
            <span class="font-mono font-bold text-indigo-400">
              {{ pendingKillConfirm.entry.siblingPorts.map((p) => `:${p}`).join(', ') }}
            </span>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-slate-400">所属用户：</span>
            <span class="font-mono">{{ pendingKillConfirm.entry.user }}</span>
          </div>
          <div
            v-if="pendingKillConfirm.entry.siblingPorts.length > 1"
            class="text-[11px] text-amber-400 pt-1"
          >
            注意：该进程同时关联了 {{ pendingKillConfirm.entry.siblingPorts.length }} 个端口，终止后将一并关闭。
          </div>
        </div>

        <div class="flex items-center justify-end gap-2 text-xs">
          <button
            type="button"
            class="btn-secondary px-3.5 py-2"
            @click="pendingKillConfirm = null"
          >
            取消 (Esc)
          </button>

          <button
            v-if="pendingKillConfirm.needAdmin"
            type="button"
            :disabled="isKilling"
            class="px-4 py-2 rounded-xl font-semibold text-white bg-amber-600 hover:bg-amber-500 cursor-pointer transition-colors flex items-center gap-1.5"
            @click="confirmAndExecuteKill(true)"
          >
            <IconRenderer name="ShieldAlert" :size="13" />
            <span>{{ isKilling ? '正在请求授权...' : '使用管理员权限终止' }}</span>
          </button>

          <button
            v-else
            type="button"
            :disabled="isKilling"
            class="px-4 py-2 rounded-xl font-semibold text-white cursor-pointer transition-colors flex items-center gap-1.5"
            :class="
              pendingKillConfirm.force
                ? 'bg-rose-600 hover:bg-rose-500'
                : 'bg-indigo-600 hover:bg-indigo-500'
            "
            @click="confirmAndExecuteKill(false)"
          >
            <span>
              {{
                isKilling
                  ? '正在终止...'
                  : pendingKillConfirm.force
                    ? '立即强制终止 (-9)'
                    : '确认停止进程 (-15)'
              }}
            </span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
