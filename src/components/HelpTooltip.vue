<script setup lang="ts">
import { ref } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import IconRenderer from './IconRenderer.vue'

withDefaults(
  defineProps<{
    title?: string
    content: string
    tip?: string
    placement?: 'top' | 'right'
  }>(),
  {
    title: '',
    tip: '',
    placement: 'right'
  }
)

const store = useLauncherStore()
const isOpen = ref(false)
</script>

<template>
  <span
    class="relative inline-flex items-center align-middle select-none"
    @mouseenter="isOpen = true"
    @mouseleave="isOpen = false"
    @focusin="isOpen = true"
    @focusout="isOpen = false"
  >
    <button
      type="button"
      tabindex="-1"
      :aria-label="title || content"
      class="inline-flex items-center justify-center w-4 h-4 rounded-full transition-colors cursor-help outline-none"
      :class="
        isOpen
          ? 'text-indigo-500 bg-indigo-500/15'
          : 'text-slate-400 hover:text-indigo-500 hover:bg-indigo-500/10'
      "
    >
      <IconRenderer name="HelpCircle" :size="13" />
    </button>

    <Transition
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0 scale-95 translate-y-0.5"
      enter-to-class="opacity-100 scale-100 translate-y-0"
      leave-active-class="transition duration-100 ease-in"
      leave-from-class="opacity-100 scale-100 translate-y-0"
      leave-to-class="opacity-0 scale-95 translate-y-0.5"
    >
      <div
        v-if="isOpen"
        role="tooltip"
        class="absolute z-50 w-[270px] p-3 rounded-xl border shadow-xl text-left pointer-events-none leading-relaxed font-normal"
        :class="[
          placement === 'right'
            ? 'left-6 top-1/2 -translate-y-1/2'
            : 'bottom-6 left-1/2 -translate-x-1/2',
          store.isDark
            ? 'bg-[#141C2F]/98 border-slate-700/90 text-slate-200 shadow-black/50'
            : 'bg-slate-900/95 border-slate-800 text-slate-100 shadow-slate-900/25'
        ]"
      >
        <div
          v-if="title"
          class="text-[12px] font-semibold text-indigo-300 mb-1 flex items-center gap-1.5"
        >
          <span class="w-1.5 h-1.5 rounded-full bg-indigo-400 shrink-0"></span>
          <span>{{ title }}</span>
        </div>
        <div class="text-[11.5px] text-slate-200/95 whitespace-normal">
          {{ content }}
        </div>
        <div
          v-if="tip"
          class="mt-1.5 pt-1.5 border-t border-slate-700/70 text-[10.5px] text-slate-400 font-mono whitespace-normal"
        >
          {{ tip }}
        </div>
      </div>
    </Transition>
  </span>
</template>
