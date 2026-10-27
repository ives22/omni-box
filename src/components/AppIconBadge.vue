<script setup lang="ts">
import { computed } from 'vue'
import IconRenderer from './IconRenderer.vue'

const props = withDefaults(
  defineProps<{
    id?: string
    icon?: string
    iconDataUrl?: string | null
    category?: string
    size?: 'sm' | 'md' | 'lg'
  }>(),
  {
    id: '',
    icon: 'AppWindow',
    iconDataUrl: null,
    category: 'plugin',
    size: 'md'
  }
)

// Refined compact icon sizes per user feedback
const sizeClasses = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'w-6 h-6 rounded-[7px]'
    case 'lg':
      return 'w-9 h-9 rounded-[10px]'
    default:
      return 'w-8 h-8 rounded-[9px]'
  }
})

const iconPixelSize = computed(() => {
  switch (props.size) {
    case 'sm':
      return 13
    case 'lg':
      return 18
    default:
      return 16
  }
})

// Distinct vibrant multi-stop gradients per plugin/tool
const gradientStyle = computed(() => {
  const id = props.id || ''
  if (id.includes('utools-hub')) {
    return 'bg-gradient-to-br from-fuchsia-500 via-purple-500 to-indigo-500 text-white shadow-sm shadow-purple-500/20'
  }
  if (id.includes('calc-pad') || props.category === 'calc') {
    return 'bg-gradient-to-br from-amber-400 via-orange-500 to-red-500 text-white shadow-sm shadow-orange-500/20'
  }
  if (id.includes('json-studio') || props.category === 'clipboard') {
    return 'bg-gradient-to-br from-violet-500 via-purple-500 to-fuchsia-500 text-white shadow-sm shadow-purple-500/20'
  }
  if (id.includes('diff-viewer')) {
    return 'bg-gradient-to-br from-emerald-400 via-teal-500 to-cyan-600 text-white shadow-sm shadow-teal-500/20'
  }
  if (id.includes('authenticator') || props.category === 'totp') {
    return 'bg-gradient-to-br from-sky-400 via-blue-500 to-indigo-600 text-white shadow-sm shadow-blue-500/20'
  }
  if (id.includes('password-vault')) {
    return 'bg-gradient-to-br from-rose-500 via-pink-600 to-red-600 text-white shadow-sm shadow-rose-500/20'
  }
  if (id.includes('regex')) {
    return 'bg-gradient-to-br from-amber-400 via-yellow-500 to-orange-500 text-white shadow-sm shadow-amber-500/20'
  }
  if (id.includes('timestamp')) {
    return 'bg-gradient-to-br from-cyan-400 via-sky-500 to-blue-600 text-white shadow-sm shadow-sky-500/20'
  }
  if (id.includes('user-center')) {
    return 'bg-gradient-to-br from-slate-600 via-slate-700 to-slate-900 text-white shadow-sm'
  }
  if (props.category === 'command') {
    if (id.includes('trash')) {
      return 'bg-gradient-to-br from-red-400 to-rose-600 text-white shadow-sm'
    }
    if (id.includes('lock')) {
      return 'bg-gradient-to-br from-indigo-500 to-slate-800 text-white shadow-sm'
    }
    if (id.includes('dark')) {
      return 'bg-gradient-to-br from-slate-700 to-slate-950 text-amber-300 shadow-sm'
    }
    return 'bg-gradient-to-br from-cyan-500 to-blue-600 text-white shadow-sm'
  }
  if (props.category === 'file') {
    return 'bg-gradient-to-br from-sky-400 to-blue-500 text-white shadow-sm'
  }
  return 'bg-gradient-to-br from-slate-700 via-slate-800 to-slate-900 text-white shadow-sm'
})
</script>

<template>
  <!-- Case 1: Real extracted macOS .icns PNG icon -->
  <div
    v-if="iconDataUrl"
    class="relative flex items-center justify-center shrink-0 select-none transition-transform duration-150 group-hover:scale-105"
    :class="sizeClasses"
  >
    <img
      :src="iconDataUrl"
      alt=""
      class="w-full h-full object-contain drop-shadow-sm"
    />
  </div>

  <!-- Case 2: Rich Multi-Stop Gradient Squircle Icon -->
  <div
    v-else
    class="relative flex items-center justify-center shrink-0 select-none overflow-hidden transition-transform duration-150 group-hover:scale-105 shadow-[inset_0_1px_1px_rgba(255,255,255,0.4)]"
    :class="[sizeClasses, gradientStyle]"
  >
    <div
      class="absolute inset-x-0 top-0 h-1/2 bg-gradient-to-b from-white/20 to-transparent pointer-events-none"
    ></div>

    <!-- Custom Isometric Cube Icon for Plugin Market -->
    <svg
      v-if="id?.includes('utools-hub')"
      :width="iconPixelSize"
      :height="iconPixelSize"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      class="relative z-10 drop-shadow-sm"
    >
      <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
      <polyline points="3.27 6.96 12 12.01 20.73 6.96" />
      <line x1="12" y1="22.08" x2="12" y2="12" />
    </svg>

    <!-- Custom Window Code Icon for Diff Viewer -->
    <svg
      v-else-if="id?.includes('diff-viewer')"
      :width="iconPixelSize"
      :height="iconPixelSize"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      class="relative z-10 drop-shadow-sm"
    >
      <rect x="3" y="4" width="18" height="16" rx="3" />
      <line x1="3" y1="9" x2="21" y2="9" />
      <path d="m9.5 13-2 2 2 2" />
      <path d="m14.5 13 2 2-2 2" />
      <line x1="12.5" y1="12.5" x2="11.5" y2="17.5" />
    </svg>

    <IconRenderer
      v-else
      :name="icon"
      :size="iconPixelSize"
      :stroke-width="2"
      class-name="relative z-10 drop-shadow-sm"
    />
  </div>
</template>
