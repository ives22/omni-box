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
        class="absolute z-50 w-[276px] p-3 rounded-xl border text-left pointer-events-none leading-relaxed font-normal backdrop-blur-md"
        :class="[
          placement === 'right'
            ? 'left-6 top-1/2 -translate-y-1/2'
            : 'bottom-6 left-1/2 -translate-x-1/2',
          store.isDark
            ? 'bg-[#141C2F]/98 border-slate-700/90 text-slate-200 shadow-xl shadow-black/50'
            : 'bg-white/98 border-slate-200/95 text-slate-700 shadow-[0_10px_30px_-5px_rgba(15,23,42,0.14),0_4px_12px_-2px_rgba(79,70,229,0.08)]'
        ]"
      >
        <div
          v-if="title"
          class="text-[12px] font-semibold mb-1 flex items-center gap-1.5"
          :class="store.isDark ? 'text-indigo-300' : 'text-indigo-600'"
        >
          <span
            class="w-1.5 h-1.5 rounded-full shrink-0"
            :class="store.isDark ? 'bg-indigo-400' : 'bg-indigo-500'"
          ></span>
          <span>{{ title }}</span>
        </div>
        <div
          class="text-[11.5px] whitespace-normal"
          :class="store.isDark ? 'text-slate-200/95' : 'text-slate-600'"
        >
          {{ content }}
        </div>
        <div
          v-if="tip"
          class="mt-2 pt-1.5 border-t text-[10.5px] font-mono whitespace-normal"
          :class="
            store.isDark
              ? 'border-slate-700/70 text-slate-400'
              : 'border-slate-100 text-slate-400'
          "
        >
          {{ tip }}
        </div>
      </div>
    </Transition>
  </span>
</template>
