<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import { CORE_PLUGIN_ITEMS, SearchResultItem } from '@/bridge/backend'
import AppIconBadge from './AppIconBadge.vue'

const emit = defineEmits<{
  (e: 'expand-change', expanded: boolean): void
}>()

const store = useLauncherStore()
const isRecentExpanded = ref(false)
const activeGridId = ref<string>('plugin:utools-hub')

const isListMode = computed(() => store.userSettings.searchBarMode === '列表模式')

onMounted(() => {
  // Ensure latest clean application titles are synced from backend
  store.refreshSystemApps(false)
})

watch(isRecentExpanded, (val) => {
  emit('expand-change', val)
})

function shortLabel(title: string): string {
  return title
    .replace(/\s*\([^)]*\)/g, '')
    .replace('uTools 插件中心与沙箱挂载', '插件市场')
    .replace('插件应用市场', '插件市场')
    .replace('2FA 验证码管理', '2FA 令牌')
    .replace('文本对比', '文本比对')
    .replace('VS Code 编辑器', 'VS Code')
    .replace('Google Chrome 浏览器', 'Chrome')
    .replace('Safari 浏览器', 'Safari')
    .replace('iTerm2 终端', 'iTerm2')
    .trim()
}

// Build "Recently Used" items list (Plugin Market first, then top used + local macOS apps)
const allRecentCandidates = computed<SearchResultItem[]>(() => {
  const frecency = store.usageStatsMap
  const autoPin = store.userSettings.autoPinFeature !== false
  const disabledSet = new Set(store.userSettings.disabledAppIds || [])

  const pool: SearchResultItem[] = []

  // 1. Place "插件市场" at the first slot
  const marketPlugin = CORE_PLUGIN_ITEMS.find((p) => p.id === 'plugin:utools-hub')
  if (marketPlugin) {
    pool.push({
      ...marketPlugin,
      title: '插件市场',
      score: 99999,
      match_type: 'default'
    })
  }

  // 2. Core built-in productivity tools
  let idx = 0
  for (const p of CORE_PLUGIN_ITEMS) {
    if (p.id === 'plugin:utools-hub' || p.id === 'plugin:user-center') continue
    idx++
    pool.push({
      ...p,
      title: shortLabel(p.title),
      score: autoPin ? 500 + (frecency[p.id] || 0) * 50 : 500 - idx,
      match_type: 'default'
    })
  }

  // 3. Local scanned macOS applications
  for (const app of store.systemApps) {
    if (disabledSet.has(app.id)) continue
    idx++
    pool.push({
      id: app.id,
      title: shortLabel(app.title),
      subtitle: app.subtitle,
      category: 'app',
      icon: app.icon || 'AppWindow',
      icon_data_url: app.icon_data_url || null,
      keywords: app.keywords,
      action_payload: app.action_payload,
      score: autoPin ? 300 + (frecency[app.id] || 0) * 50 : 300 - idx,
      match_type: 'default'
    })
  }

  const [first, ...rest] = pool
  if (autoPin) {
    rest.sort((a, b) => b.score - a.score)
  }
  return first ? [first, ...rest] : rest
})

const visibleRecentItems = computed(() => {
  const defaultLimit = isListMode.value ? 6 : 10
  return isRecentExpanded.value
    ? allRecentCandidates.value
    : allRecentCandidates.value.slice(0, defaultLimit)
})
</script>

<template>
  <section
    aria-label="最近使用面板"
    class="px-4 pt-2.5 pb-3.5 select-none"
    :class="[
      isRecentExpanded ? 'flex-1 h-full overflow-y-auto' : 'overflow-hidden',
      store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'
    ]"
  >
    <div class="flex items-center justify-between mb-1.5 px-1">
      <div class="flex items-center gap-2">
        <h2
          class="text-[12px] font-semibold tracking-tight"
          :class="store.isDark ? 'text-slate-300' : 'text-slate-700'"
        >
          最近使用
        </h2>
        <span
          v-if="isListMode"
          class="text-[10px] px-1.5 py-0.2 rounded font-mono"
          :class="
            store.isDark
              ? 'bg-indigo-500/15 text-indigo-300'
              : 'bg-indigo-50 text-indigo-600'
          "
        >
          列表模式
        </span>
      </div>
      <button
        type="button"
        class="text-[11px] font-normal cursor-pointer transition-colors px-1.5 py-0.5 rounded hover:bg-slate-200/60 dark:hover:bg-slate-800/60"
        :class="
          store.isDark
            ? 'text-slate-400 hover:text-slate-200'
            : 'text-slate-500 hover:text-slate-800'
        "
        @mousedown.prevent
        @click.stop="isRecentExpanded = !isRecentExpanded"
      >
        {{ isRecentExpanded ? '收起' : `展开 (${allRecentCandidates.length})` }}
      </button>
    </div>

    <!-- Mode A: 10-Column Compact Icon Grid (聚合模式) -->
    <div v-if="!isListMode" class="grid grid-cols-10 gap-1">
      <button
        v-for="item in visibleRecentItems"
        :key="item.id"
        type="button"
        :title="item.subtitle || item.title"
        class="group flex flex-col items-center justify-center px-1.5 py-2 rounded-xl cursor-pointer transition-all duration-150 outline-none focus:ring-1 focus:ring-indigo-500/40"
        :class="[
          activeGridId === item.id
            ? store.isDark
              ? 'bg-slate-800/85'
              : 'bg-slate-200/70'
            : store.isDark
              ? 'hover:bg-slate-800/45'
              : 'hover:bg-slate-200/45'
        ]"
        @mouseenter="activeGridId = item.id"
        @click="store.executeItem(item)"
      >
        <AppIconBadge
          :id="item.id"
          :icon="item.icon"
          :icon-data-url="item.icon_data_url"
          :category="item.category"
          size="lg"
        />
        <span
          class="mt-1.5 w-full text-[11px] leading-tight font-normal text-center truncate"
          :class="store.isDark ? 'text-slate-300' : 'text-slate-700'"
        >
          {{ item.title }}
        </span>
      </button>
    </div>

    <!-- Mode B: Vertical Detail List (列表模式) -->
    <div v-else class="space-y-1">
      <button
        v-for="item in visibleRecentItems"
        :key="item.id"
        type="button"
        class="w-full flex items-center justify-between px-3 py-2 rounded-xl text-left cursor-pointer transition-all duration-150"
        :class="[
          activeGridId === item.id
            ? store.isDark
              ? 'bg-indigo-500/15 text-white'
              : 'bg-indigo-50/90 text-slate-900'
            : store.isDark
              ? 'hover:bg-slate-800/50 text-slate-200'
              : 'hover:bg-slate-200/50 text-slate-800'
        ]"
        @mouseenter="activeGridId = item.id"
        @click="store.executeItem(item)"
      >
        <div class="flex items-center gap-3 min-w-0 pr-3">
          <AppIconBadge
            :id="item.id"
            :icon="item.icon"
            :icon-data-url="item.icon_data_url"
            :category="item.category"
            size="md"
          />
          <div class="min-w-0">
            <div class="text-[13px] font-semibold truncate">{{ item.title }}</div>
            <div class="text-[11px] text-slate-400 truncate">{{ item.subtitle }}</div>
          </div>
        </div>
        <span
          class="text-[10px] px-2 py-0.5 rounded font-mono shrink-0"
          :class="
            store.isDark
              ? 'bg-slate-800 text-slate-400'
              : 'bg-slate-200/80 text-slate-600'
          "
        >
          {{ item.category === 'plugin' ? '内置插件' : '本机应用' }}
        </span>
      </button>
    </div>
  </section>
</template>
