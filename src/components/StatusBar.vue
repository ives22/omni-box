<script setup lang="ts">
import { useLauncherStore } from '@/stores/launcher'
import IconRenderer from './IconRenderer.vue'

const store = useLauncherStore()
</script>

<template>
  <footer
    class="flex items-center justify-between h-[34px] px-6 text-[11px] border-t select-none transition-colors duration-150"
    :class="
      store.isDark
        ? 'bg-obsidian-950/80 border-slate-800/60 text-slate-400'
        : 'bg-[#F0F2F5]/90 border-slate-200/70 text-slate-500'
    "
  >
    <div class="flex items-center gap-2.5">
      <span class="w-1.5 h-1.5 rounded-full bg-emerald-500"></span>
      <span>已索引 {{ store.systemApps.length }} 款本机应用</span>
      <span class="opacity-40">•</span>
      <span>输入 <code class="font-mono">f 关键词</code> 搜文件</span>
    </div>

    <div class="flex items-center gap-3">
      <button
        type="button"
        :title="store.isDark ? '切换为浅色主题' : '切换为深色主题'"
        aria-label="切换深浅色主题"
        class="flex items-center gap-1 px-2 py-0.5 rounded hover:bg-slate-500/10 cursor-pointer transition-colors"
        @click="store.toggleTheme()"
      >
        <IconRenderer :name="store.isDark ? 'Moon' : 'Sun'" :size="12" />
        <span>{{ store.isDark ? '深色' : '浅色' }}</span>
      </button>
    </div>
  </footer>
</template>
