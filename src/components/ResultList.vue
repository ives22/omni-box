<script setup lang="ts">
import { useLauncherStore } from '@/stores/launcher'
import IconRenderer from './IconRenderer.vue'
import AppIconBadge from './AppIconBadge.vue'

const store = useLauncherStore()

function formatTotpDisplay(code?: string): string {
  if (!code || code.length < 6) return code || '------'
  return `${code.slice(0, 3)} ${code.slice(3)}`
}

function cleanTitle(title: string): string {
  return title.replace('uTools 插件中心与沙箱挂载', '插件市场')
}
</script>

<template>
  <section
    aria-label="搜索候选列表"
    class="max-h-[480px] overflow-y-auto px-3 py-2.5 space-y-0.5 select-none"
    :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
    @mousedown.prevent
  >
    <!-- Empty State -->
    <div
      v-if="store.results.length === 0"
      class="flex flex-col items-center justify-center py-9 text-center"
    >
      <div
        class="flex items-center justify-center w-9 h-9 mb-2 rounded-xl border"
        :class="
          store.isDark
            ? 'bg-slate-800/50 border-slate-700/60 text-slate-400'
            : 'bg-slate-100 border-slate-200 text-slate-500'
        "
      >
        <IconRenderer name="Search" :size="16" />
      </div>
      <p class="text-xs font-medium" :class="store.isDark ? 'text-slate-300' : 'text-slate-700'">
        未找到与 “{{ store.searchQuery }}” 匹配的应用或工具
      </p>
      <p class="mt-0.5 text-[11px] text-slate-500">
        输入 <code class="font-mono text-indigo-500">f {{ store.searchQuery }}</code> 搜索本地文件
      </p>
    </div>

    <!-- Compact Result Items -->
    <div
      v-for="(item, idx) in store.results"
      :key="item.id"
      role="option"
      :aria-selected="idx === store.selectedIndex"
      class="group relative flex items-center justify-between px-3 py-1.5 rounded-xl cursor-pointer transition-all duration-100"
      :class="[
        idx === store.selectedIndex
          ? store.isDark
            ? 'bg-indigo-500/20 text-white ring-1 ring-indigo-500/35'
            : 'bg-indigo-50/95 text-indigo-950 ring-1 ring-indigo-500/25 shadow-[0_1px_2px_rgba(79,70,229,0.06)]'
          : store.isDark
            ? 'hover:bg-slate-800/40 text-slate-300'
            : 'hover:bg-slate-200/40 text-slate-700'
      ]"
      @mouseenter="store.selectedIndex = idx"
      @click="store.executeItem(item, false)"
    >
      <div class="flex items-center gap-2.5 min-w-0 flex-1">
        <AppIconBadge
          :id="item.id"
          :icon="item.icon"
          :icon-data-url="item.icon_data_url"
          :category="item.category"
          size="md"
        />

        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <span
              class="text-[13px] font-medium truncate"
              :class="[
                item.category === 'calc' ? 'font-mono text-sm text-indigo-500 dark:text-indigo-400 font-semibold' : '',
                idx === store.selectedIndex
                  ? store.isDark
                    ? 'text-white'
                    : 'text-slate-900'
                  : store.isDark
                    ? 'text-slate-100'
                    : 'text-slate-800'
              ]"
            >
              {{ cleanTitle(item.title) }}
            </span>
            <span
              v-if="item.badge === '应用推荐'"
              class="px-1.5 py-0.2 text-[10px] font-medium rounded bg-amber-500/15 text-amber-500 border border-amber-500/30 shrink-0"
            >
              应用推荐
            </span>
            <span
              v-else-if="
                item.badge === '系统指令' ||
                item.badge === '指令直达' ||
                item.badge === '算式直达' ||
                item.badge === 'JSON 感知'
              "
              class="px-1.5 py-0.2 text-[10px] font-medium rounded bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border border-emerald-500/30 shrink-0"
            >
              {{ item.badge }}
            </span>
            <span
              v-else-if="item.badge === '英汉速译' || item.badge === '快速翻译' || item.badge === 'AI 助手'"
              class="px-1.5 py-0.2 text-[10px] font-medium rounded bg-indigo-500/15 text-indigo-500 dark:text-indigo-400 border border-indigo-500/30 shrink-0"
            >
              {{ item.badge }}
            </span>
          </div>

          <p
            class="text-[11px] truncate"
            :class="store.isDark ? 'text-slate-400' : 'text-slate-500'"
          >
            {{ item.subtitle }}
          </p>
        </div>
      </div>

      <!-- Right Action / Direct 2FA Live Code Preview -->
      <div class="flex items-center gap-2 ml-2 shrink-0">
        <div
          v-if="item.category === 'totp' && item.totpCode"
          class="flex items-center gap-1.5 px-2.5 py-0.5 rounded-lg border font-mono"
          :class="
            store.isDark
              ? 'bg-slate-900 border-indigo-500/30 text-indigo-300'
              : 'bg-indigo-50 border-indigo-200 text-indigo-700'
          "
        >
          <span class="text-sm font-bold tracking-wider">
            {{ formatTotpDisplay(item.totpCode) }}
          </span>
          <span class="text-[10px] opacity-75">{{ item.totpRemaining }}s</span>
        </div>

        <button
          v-if="idx === store.selectedIndex && (item.category === 'app' || item.category === 'file')"
          type="button"
          title="在访达 (Finder) 中显示 (⌘ + Enter)"
          class="hidden sm:flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-slate-700/80 text-slate-200 hover:bg-slate-700'
              : 'bg-white text-slate-600 shadow-sm'
          "
          @click.stop="store.executeItem(item, true)"
        >
          <span>定位</span>
          <span class="font-mono opacity-70">⌘↵</span>
        </button>

        <span v-if="idx === store.selectedIndex" class="kbd-badge text-[10px]">
          ↵
        </span>
      </div>
    </div>
  </section>
</template>
