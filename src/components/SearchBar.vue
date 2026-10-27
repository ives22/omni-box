<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import IconRenderer from './IconRenderer.vue'
import AppIconBadge from './AppIconBadge.vue'

const store = useLauncherStore()
const mainInputRef = ref<HTMLInputElement | null>(null)
const subInputRef = ref<HTMLInputElement | null>(null)
const showMoreMenu = ref(false)
const contextMenuPos = ref<{ x: number; y: number } | null>(null)

// Screenshot 1 vs Screenshot 2 Distinction:
// 1. Embedded Plugin Mode (Screenshot 1): Inside a tool in the main launcher window BEFORE Command+D detach
const isEmbeddedPluginMode = computed(() => {
  return (
    Boolean(store.activePluginMeta) &&
    store.activePluginId !== 'user-center' &&
    !store.isStandaloneWindow &&
    !store.isDetachedWindow
  )
})

// 2. Standalone / Detached Tool Window Mode (Screenshot 2): After Command+D or context-menu detach
const isStandalonePluginMode = computed(() => {
  return (
    Boolean(store.activePluginMeta) &&
    store.activePluginId !== 'user-center' &&
    (store.isStandaloneWindow || store.isDetachedWindow)
  )
})

// 3. Show left macOS Traffic Lights (Red/Yellow/Green) ONLY in Standalone/Detached mode or User Center
const showTrafficLights = computed(() => {
  return (
    store.isStandaloneWindow ||
    store.isDetachedWindow ||
    store.activePluginId === 'user-center'
  )
})

const detachShortcutLabel = computed(() => {
  const raw = store.userSettings.detachWindowHotkey || 'Command+D'
  return raw
    .replace(/Command\s*\+?\s*/i, '⌘')
    .replace(/Option\s*\+?\s*/i, '⌥')
    .replace(/Ctrl\s*\+?\s*/i, '⌃')
    .replace(/Shift\s*\+?\s*/i, '⇧')
})

function doFocusInputElement() {
  const target = store.activePluginId ? subInputRef.value : mainInputRef.value
  if (target) {
    target.focus({ preventScroll: true })
  }
}

function focusActiveInput() {
  doFocusInputElement()
  nextTick(() => {
    doFocusInputElement()
    requestAnimationFrame(() => {
      doFocusInputElement()
    })
    setTimeout(doFocusInputElement, 20)
    setTimeout(doFocusInputElement, 65)
  })
}

function selectActiveInput() {
  focusActiveInput()
  nextTick(() => {
    setTimeout(() => {
      const target = store.activePluginId ? subInputRef.value : mainInputRef.value
      target?.select()
    }, 25)
  })
}

watch(
  () => store.activePluginId,
  () => {
    showMoreMenu.value = false
    contextMenuPos.value = null
    focusActiveInput()
  }
)

watch(
  () => store.searchQuery,
  () => {
    if (!store.activePluginId && document.activeElement !== mainInputRef.value) {
      mainInputRef.value?.focus()
    }
  }
)

// Smart Drag Handler: Only start window drag when mouse actually moves > 4px while held down
function handleHeaderMouseDown(e: MouseEvent) {
  if (e.button !== 0) return
  const target = e.target as HTMLElement | null
  if (target?.closest('button') || target?.closest('input') || target?.closest('.menu-popup')) {
    return
  }

  showMoreMenu.value = false
  contextMenuPos.value = null

  const startX = e.screenX
  const startY = e.screenY

  function onMouseMove(moveEvent: MouseEvent) {
    const dist = Math.hypot(moveEvent.screenX - startX, moveEvent.screenY - startY)
    if (dist > 4) {
      cleanup()
      store.startDraggingWindow()
    }
  }

  function cleanup() {
    window.removeEventListener('mousemove', onMouseMove)
    window.removeEventListener('mouseup', cleanup)
  }

  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', cleanup)
}

function handleHeaderDoubleClick(e: MouseEvent) {
  const target = e.target as HTMLElement | null
  if (target?.closest('button') || target?.closest('input')) return
  store.toggleWindowMaximize()
}

function handleHeaderContextMenu(e: MouseEvent) {
  if (!store.activePluginMeta) return
  e.preventDefault()
  showMoreMenu.value = false
  contextMenuPos.value = {
    x: Math.min(e.clientX, window.innerWidth - 210),
    y: 48
  }
}

function closeMenus() {
  showMoreMenu.value = false
  contextMenuPos.value = null
}

function handleDetachTrigger() {
  closeMenus()
  store.detachCurrentPluginToWindow()
}

function handleTrafficClose() {
  closeMenus()
  if (store.isStandaloneWindow) {
    store.closeCurrentWindow()
    return
  }
  store.closePlugin()
  store.isDetachedWindow = false
  store.isWindowMaximized = false
  store.hideAppWindow()
}

function handleKeyDown(e: KeyboardEvent) {
  if (!store.activePluginId) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      if (store.results.length > 0) {
        store.selectedIndex = (store.selectedIndex + 1) % store.results.length
      }
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      if (store.results.length > 0) {
        store.selectedIndex =
          (store.selectedIndex - 1 + store.results.length) % store.results.length
      }
    } else if (e.key === 'Enter') {
      e.preventDefault()
      const current = store.results[store.selectedIndex]
      if (current) {
        store.executeItem(current, e.metaKey || e.ctrlKey)
      }
    } else if (
      e.key === ' ' &&
      store.userSettings.spaceToOpen &&
      store.searchQuery.trim() &&
      !/^(f|find)$/i.test(store.searchQuery.trim())
    ) {
      e.preventDefault()
      const current = store.results[store.selectedIndex]
      if (current) {
        store.executeItem(current, e.metaKey || e.ctrlKey)
      }
    } else if (e.key === 'Tab') {
      const current = store.results[store.selectedIndex]
      if (current && current.id === 'quick:math') {
        e.preventDefault()
        store.pluginInitialPayload = store.searchQuery
        store.openPlugin('calc-pad')
      }
    } else if (e.key === 'Escape') {
      e.preventDefault()
      const nativeEsc = (window as unknown as { __omniboxNativeEscape?: () => void })
        .__omniboxNativeEscape
      if (nativeEsc) {
        nativeEsc()
      } else {
        store.searchQuery = ''
        store.isDetachedWindow = false
        store.isWindowMaximized = false
        store.hideAppWindow()
      }
    }
  } else {
    if (e.key === 'Escape') {
      e.preventDefault()
      if (showMoreMenu.value || contextMenuPos.value) {
        closeMenus()
        return
      }
      if (store.isStandaloneWindow) {
        if (store.subInputQuery) {
          store.subInputQuery = ''
        }
        return
      }
      const nativeEsc = (window as unknown as { __omniboxNativeEscape?: () => void })
        .__omniboxNativeEscape
      if (nativeEsc) {
        nativeEsc()
      } else if (store.subInputQuery) {
        store.subInputQuery = ''
      } else {
        store.closePlugin()
      }
    }
  }
}

function toggleUserCenter() {
  closeMenus()
  if (store.activePluginId === 'user-center') {
    store.closePlugin()
  } else {
    store.openPlugin('user-center')
  }
}

function handleDocumentClick(e: MouseEvent) {
  const target = e.target as HTMLElement | null
  if (!target?.closest('.menu-popup') && !target?.closest('.menu-trigger-btn')) {
    closeMenus()
  }
}

onMounted(() => {
  focusActiveInput()
  window.addEventListener('focus', focusActiveInput)
  window.addEventListener('click', handleDocumentClick)
})

onUnmounted(() => {
  window.removeEventListener('focus', focusActiveInput)
  window.removeEventListener('click', handleDocumentClick)
})

defineExpose({
  focusActiveInput,
  selectActiveInput
})
</script>

<template>
  <header
    data-tauri-drag-region
    class="relative flex items-center h-[52px] px-4 border-b select-none transition-colors duration-150 shrink-0 z-50"
    :class="
      store.isDark
        ? 'bg-[#0E131F] border-slate-800 text-slate-100'
        : 'bg-[#F3F4F6] border-slate-200/90 text-slate-900'
    "
    @mousedown="handleHeaderMouseDown"
    @dblclick="handleHeaderDoubleClick"
    @contextmenu="handleHeaderContextMenu"
  >
    <!-- ========================================================================= -->
    <!-- Left macOS Traffic Lights (Red / Yellow / Green)                          -->
    <!-- Shown ONLY in Detached/Standalone Window Mode (Screenshot 2) or Settings  -->
    <!-- ========================================================================= -->
    <div
      v-if="showTrafficLights"
      class="group/lights flex items-center gap-2 mr-3.5 shrink-0"
    >
      <!-- Red: Close Window -->
      <button
        type="button"
        title="关闭窗口 (⌘W)"
        aria-label="关闭窗口"
        class="w-3 h-3 rounded-full bg-[#FF5F57] border border-[#E0443E] flex items-center justify-center text-[#4D0000] cursor-pointer transition-transform active:scale-90"
        @click="handleTrafficClose"
      >
        <svg
          class="w-2 h-2 opacity-0 group-hover/lights:opacity-100 transition-opacity"
          viewBox="0 0 10 10"
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
        >
          <path d="M2 2l6 6M8 2L2 8" />
        </svg>
      </button>

      <!-- Yellow: Minimize Window -->
      <button
        type="button"
        title="最小化窗口"
        aria-label="最小化窗口"
        class="w-3 h-3 rounded-full bg-[#FEBC2E] border border-[#D89E24] flex items-center justify-center text-[#5A3000] cursor-pointer transition-transform active:scale-90"
        @click="store.minimizeAppWindow()"
      >
        <svg
          class="w-2 h-2 opacity-0 group-hover/lights:opacity-100 transition-opacity"
          viewBox="0 0 10 10"
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
        >
          <path d="M2 5h6" />
        </svg>
      </button>

      <!-- Green: Maximize / Restore Window -->
      <button
        type="button"
        :title="store.isWindowMaximized ? '还原默认窗口尺寸' : '放大窗口'"
        :aria-label="store.isWindowMaximized ? '还原窗口尺寸' : '放大窗口'"
        class="w-3 h-3 rounded-full bg-[#28C840] border border-[#1AAB29] flex items-center justify-center text-[#003D00] cursor-pointer transition-transform active:scale-90"
        @click="store.toggleWindowMaximize()"
      >
        <svg
          class="w-2 h-2 opacity-0 group-hover/lights:opacity-100 transition-opacity"
          viewBox="0 0 10 10"
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
        >
          <path v-if="!store.isWindowMaximized" d="M5 2v6M2 5h6" />
          <path v-else d="M2 5h6" />
        </svg>
      </button>
    </div>

    <!-- ========================================================================= -->
    <!-- Mode 1: Main Launcher Search Bar                                          -->
    <!-- ========================================================================= -->
    <template v-if="!store.activePluginMeta">
      <div
        class="flex items-center flex-1 h-[34px] transition-all"
        :class="
          showTrafficLights
            ? [
                'max-w-[380px] px-3 rounded-lg border',
                store.isDark
                  ? 'bg-[#182032] border-slate-700/80'
                  : 'bg-[#E5E7EB]/85 border-slate-300/60'
              ]
            : 'h-full'
        "
      >
        <input
          ref="mainInputRef"
          v-model="store.searchQuery"
          type="text"
          autocomplete="off"
          spellcheck="false"
          :placeholder="
            store.userSettings.searchPlaceholder || '搜索功能 / 本机应用 / 粘贴文本或输入算式'
          "
          aria-label="全局命令搜索框"
          class="flex-1 h-full bg-transparent outline-none placeholder:text-slate-400/90 dark:placeholder:text-slate-500"
          :class="showTrafficLights ? 'text-[13.5px]' : 'text-[15px]'"
          @keydown="handleKeyDown"
        />

        <button
          v-if="store.searchQuery"
          type="button"
          title="清空输入 (Esc)"
          aria-label="清空搜索输入"
          class="flex items-center justify-center w-5 h-5 ml-1 rounded-full text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
          @click="
            () => {
              store.searchQuery = ''
              focusActiveInput()
            }
          "
        >
          <IconRenderer name="X" :size="13" />
        </button>
      </div>

      <div v-if="showTrafficLights" class="flex-1 h-full cursor-default"></div>
    </template>

    <!-- ========================================================================= -->
    <!-- Mode 2A: Standalone Detached Tool Window Header (Matches Screenshot 2!)   -->
    <!-- Left: [Traffic Lights] + [Colored Tool Icon] + [Tool Title]               -->
    <!-- ========================================================================= -->
    <template v-else-if="isStandalonePluginMode">
      <div class="flex items-center gap-2.5 mr-4 shrink-0">
        <AppIconBadge
          :id="store.activePluginMeta.id"
          :icon="store.activePluginMeta.icon"
          size="sm"
        />
        <span class="text-[14px] font-bold tracking-tight">
          {{ store.activePluginMeta.name }}
        </span>
      </div>

      <!-- Wide Draggable Titlebar Area -->
      <div class="flex-1 h-full cursor-default"></div>

      <!-- Compact Sub-Filter Input on Right Side of Standalone Window -->
      <div
        class="flex items-center w-[240px] h-[30px] px-2.5 mr-2 rounded-lg border transition-colors"
        :class="
          store.isDark
            ? 'bg-[#182032] border-slate-700/80 focus-within:border-indigo-500/60'
            : 'bg-white/90 border-slate-300/70 focus-within:border-indigo-400'
        "
      >
        <IconRenderer name="Search" :size="12" class-name="text-slate-400 mr-1.5 shrink-0" />
        <input
          ref="subInputRef"
          v-model="store.subInputQuery"
          type="text"
          autocomplete="off"
          spellcheck="false"
          :placeholder="store.subInputPlaceholder"
          aria-label="独立窗口内搜索"
          class="flex-1 h-full bg-transparent text-[12px] outline-none placeholder:text-slate-400"
          @keydown="handleKeyDown"
        />
        <button
          v-if="store.subInputQuery"
          type="button"
          class="p-0.5 rounded text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
          @click="
            () => {
              store.subInputQuery = ''
              focusActiveInput()
            }
          "
        >
          <IconRenderer name="X" :size="11" />
        </button>
      </div>
    </template>

    <!-- ========================================================================= -->
    <!-- Mode 2B: Embedded Plugin Mode in Main Window (Matches Screenshot 1!)      -->
    <!-- OR User Center Mode                                                       -->
    <!-- ========================================================================= -->
    <template v-else>
      <!-- Active Module Badge Pill (Matches Screenshot 1 Top-Left Pill; Right-click to Detach) -->
      <button
        v-if="store.activePluginId !== 'user-center'"
        type="button"
        :title="`点击或按 Esc 返回主搜索框 · 右键或按 ${detachShortcutLabel} 分离为独立窗口`"
        aria-label="退出当前插件返回主界面"
        class="group flex items-center gap-2 pl-2 pr-2.5 py-1.5 mr-2.5 rounded-full border text-xs font-medium cursor-pointer transition-all duration-150 shrink-0"
        :class="
          store.isDark
            ? 'bg-[#182032] border-slate-700/90 text-slate-100 hover:bg-slate-800'
            : 'bg-[#E5E7EB]/85 border-slate-300/80 text-slate-800 hover:bg-slate-200/80'
        "
        @click="store.closePlugin()"
      >
        <AppIconBadge
          :id="store.activePluginMeta.id"
          :icon="store.activePluginMeta.icon"
          size="sm"
        />
        <span class="text-[13px] font-semibold tracking-tight">{{
          store.activePluginMeta.name
        }}</span>
        <span
          class="h-3.5 w-[1px] mx-0.5"
          :class="store.isDark ? 'bg-slate-700' : 'bg-slate-300'"
        ></span>
        <IconRenderer
          name="X"
          :size="13"
          class-name="opacity-55 group-hover:opacity-100 transition-opacity"
        />
      </button>

      <!-- Rounded Sub-Input Box -->
      <div
        class="flex items-center w-full max-w-[360px] h-[34px] px-3 rounded-lg border transition-colors"
        :class="
          store.isDark
            ? 'bg-[#182032] border-slate-700/80 focus-within:border-indigo-500/60'
            : 'bg-[#E5E7EB]/80 border-slate-300/50 focus-within:bg-white focus-within:border-indigo-400'
        "
      >
        <input
          ref="subInputRef"
          v-model="store.subInputQuery"
          type="text"
          autocomplete="off"
          spellcheck="false"
          :placeholder="store.subInputPlaceholder"
          aria-label="插件子输入框"
          class="flex-1 h-full bg-transparent text-[13px] outline-none placeholder:text-slate-400"
          @keydown="handleKeyDown"
        />

        <button
          v-if="store.subInputQuery"
          type="button"
          aria-label="清空子输入框"
          class="p-0.5 rounded text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
          @click="
            () => {
              store.subInputQuery = ''
              focusActiveInput()
            }
          "
        >
          <IconRenderer name="X" :size="13" />
        </button>
      </div>

      <!-- Wide Draggable Titlebar Spacer -->
      <div class="flex-1 h-full cursor-default"></div>
    </template>

    <!-- ========================================================================= -->
    <!-- Right Controls:                                                           -->
    <!-- - Screenshot 1 (Embedded Plugin): Detach Button + 3-Dots Menu + Tool Icon -->
    <!-- - Screenshot 2 (Standalone Window): Menu Button + Pin on Top (📌)         -->
    <!-- - Main Search / User Center: Pin (if detached) + User Center Robot Avatar -->
    <!-- ========================================================================= -->
    <div class="flex items-center gap-1.5 ml-2 shrink-0">
      <!-- Embedded Plugin Mode Controls (Matches Screenshot 1 Right Side) -->
      <template v-if="isEmbeddedPluginMode && store.activePluginMeta">
        <!-- Quick Detach to Standalone Window Button (Command+D) -->
        <button
          type="button"
          :title="`分离为独立窗口 (${detachShortcutLabel})`"
          aria-label="分离为独立窗口"
          class="flex items-center gap-1 px-2 h-7 rounded-lg text-[11px] font-medium border cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-slate-800/80 border-slate-700 text-slate-300 hover:text-indigo-300 hover:border-indigo-500/50'
              : 'bg-white/90 border-slate-200/90 text-slate-600 hover:text-indigo-600 hover:border-indigo-300'
          "
          @click="handleDetachTrigger"
        >
          <IconRenderer name="CornerUpRight" :size="12" />
          <span>独立窗口</span>
          <kbd class="text-[10px] opacity-65 font-mono">{{ detachShortcutLabel }}</kbd>
        </button>

        <!-- Three-Dots Menu Button (⋮) -->
        <button
          type="button"
          title="更多窗口操作 (右键或点击展开)"
          aria-label="更多窗口操作"
          class="menu-trigger-btn flex items-center justify-center w-7 h-7 rounded-lg cursor-pointer transition-colors"
          :class="
            showMoreMenu
              ? 'bg-indigo-500/15 text-indigo-500'
              : store.isDark
                ? 'text-slate-400 hover:text-slate-200 hover:bg-slate-800'
                : 'text-slate-500 hover:text-slate-800 hover:bg-slate-200/70'
          "
          @click.stop="
            () => {
              contextMenuPos = null
              showMoreMenu = !showMoreMenu
            }
          "
        >
          <svg class="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
            <circle cx="8" cy="3" r="1.4" />
            <circle cx="8" cy="8" r="1.4" />
            <circle cx="8" cy="13" r="1.4" />
          </svg>
        </button>

        <!-- Tool Large Badge Icon on Far Right (Matches Screenshot 1 Top-Right) -->
        <div
          class="ml-0.5 cursor-pointer"
          :title="`${store.activePluginMeta.name} · 按 ${detachShortcutLabel} 分离为独立窗口`"
          @click="handleDetachTrigger"
        >
          <AppIconBadge
            :id="store.activePluginMeta.id"
            :icon="store.activePluginMeta.icon"
            size="md"
          />
        </div>
      </template>

      <!-- Standalone Detached Tool Window Controls (Matches Screenshot 2 Right Side: Menu + Pin 📌) -->
      <template v-else-if="isStandalonePluginMode">
        <!-- Menu Button (≡) -->
        <button
          type="button"
          title="独立窗口菜单"
          aria-label="独立窗口菜单"
          class="menu-trigger-btn flex items-center justify-center w-7 h-7 rounded-lg cursor-pointer transition-colors"
          :class="
            showMoreMenu
              ? 'bg-indigo-500/15 text-indigo-500'
              : store.isDark
                ? 'text-slate-400 hover:text-slate-200 hover:bg-slate-800'
                : 'text-slate-500 hover:text-slate-800 hover:bg-slate-200/70'
          "
          @click.stop="
            () => {
              contextMenuPos = null
              showMoreMenu = !showMoreMenu
            }
          "
        >
          <svg class="w-4 h-4" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
            <path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
          </svg>
        </button>

        <!-- Pin Window on Top Button (📌) -->
        <button
          type="button"
          :title="store.isPinnedOnTop ? '已固定在最前端 (点击取消置顶)' : '固定独立窗口在最前端 (Pin)'"
          aria-label="切换窗口置顶"
          class="flex items-center justify-center w-7 h-7 rounded-lg cursor-pointer transition-colors"
          :class="
            store.isPinnedOnTop
              ? 'bg-indigo-500/15 text-indigo-500'
              : store.isDark
                ? 'text-slate-400 hover:text-slate-200 hover:bg-slate-800'
                : 'text-slate-500 hover:text-slate-800 hover:bg-slate-200/70'
          "
          @click="store.togglePinOnTop()"
        >
          <IconRenderer name="Pin" :size="14" />
        </button>
      </template>

      <!-- Default Main Search Bar / User Center Right Controls -->
      <template v-else>
        <button
          v-if="showTrafficLights"
          type="button"
          :title="store.isPinnedOnTop ? '取消窗口置顶' : '固定窗口在最前端 (Pin)'"
          aria-label="切换窗口置顶"
          class="flex items-center justify-center w-7 h-7 rounded-lg cursor-pointer transition-colors"
          :class="
            store.isPinnedOnTop
              ? 'bg-indigo-500/15 text-indigo-500'
              : store.isDark
                ? 'text-slate-400 hover:text-slate-200 hover:bg-slate-800'
                : 'text-slate-500 hover:text-slate-800 hover:bg-slate-200/70'
          "
          @click="store.togglePinOnTop()"
        >
          <IconRenderer name="Pin" :size="14" />
        </button>

        <button
          type="button"
          title="个人中心与设置 (⌘,)"
          aria-label="打开个人中心"
          class="group relative flex items-center justify-center w-8 h-8 rounded-full cursor-pointer transition-all duration-150 hover:scale-105 active:scale-95 shadow-sm outline-none shrink-0"
          :class="[
            store.activePluginId === 'user-center'
              ? 'ring-2 ring-indigo-500/80 bg-gradient-to-b from-indigo-500/20 to-slate-300 dark:from-indigo-500/30 dark:to-slate-800'
              : store.isDark
                ? 'bg-gradient-to-b from-slate-600 via-slate-700 to-slate-800 ring-1 ring-white/20'
                : 'bg-gradient-to-b from-slate-200 via-slate-300 to-slate-400/90 ring-1 ring-slate-300/80'
          ]"
          @click="toggleUserCenter"
        >
          <svg viewBox="0 0 40 40" class="w-7 h-7">
            <defs>
              <linearGradient id="helmetGradUnified" x1="0%" y1="0%" x2="0%" y2="100%">
                <stop offset="0%" stop-color="#FFFFFF" />
                <stop offset="100%" stop-color="#CBD5E1" />
              </linearGradient>
              <linearGradient id="visorGradUnified" x1="0%" y1="0%" x2="100%" y2="100%">
                <stop offset="0%" stop-color="#0F172A" />
                <stop offset="100%" stop-color="#1E293B" />
              </linearGradient>
            </defs>
            <path d="M8 36 C8 29, 32 29, 32 36" fill="url(#helmetGradUnified)" opacity="0.9" />
            <rect x="9" y="8" width="22" height="20" rx="10" fill="url(#helmetGradUnified)" />
            <rect x="6.5" y="15" width="3" height="6" rx="1.5" fill="#94A3B8" />
            <rect x="30.5" y="15" width="3" height="6" rx="1.5" fill="#94A3B8" />
            <rect x="12" y="13.5" width="16" height="8.5" rx="4.25" fill="url(#visorGradUnified)" />
            <path
              d="M16.5 16.5 L19 18.5 L16.5 19.5 M23.5 16.5 L21 18.5 L23.5 19.5 M20 16 L20 20"
              stroke="#6366F1"
              stroke-width="1.6"
              stroke-linecap="round"
              fill="none"
            />
          </svg>
        </button>
      </template>
    </div>

    <!-- ========================================================================= -->
    <!-- Dropdown / Right-Click Context Menu for Plugin & Standalone Window        -->
    <!-- ========================================================================= -->
    <div
      v-if="(showMoreMenu || contextMenuPos) && store.activePluginMeta"
      class="menu-popup fixed z-50 w-52 py-1.5 rounded-xl border shadow-xl text-xs select-none"
      :class="
        store.isDark
          ? 'bg-[#141B2D] border-slate-700/90 text-slate-200'
          : 'bg-white border-slate-200/90 text-slate-700'
      "
      :style="
        contextMenuPos
          ? { left: `${contextMenuPos.x}px`, top: `${contextMenuPos.y}px` }
          : { right: '14px', top: '48px' }
      "
    >
      <!-- Option 1: Detach to Standalone Window (Only when not yet standalone) -->
      <button
        v-if="!isStandalonePluginMode"
        type="button"
        class="w-full flex items-center justify-between px-3 py-2 text-left hover:bg-indigo-500/15 hover:text-indigo-500 dark:hover:text-indigo-300 cursor-pointer transition-colors"
        @click="handleDetachTrigger"
      >
        <div class="flex items-center gap-2 font-medium">
          <IconRenderer name="CornerUpRight" :size="14" class-name="text-indigo-400" />
          <span>分离为独立窗口</span>
        </div>
        <kbd
          class="px-1.5 py-0.5 text-[10px] rounded font-mono"
          :class="store.isDark ? 'bg-slate-800 text-slate-400' : 'bg-slate-100 text-slate-500'"
        >
          {{ detachShortcutLabel }}
        </kbd>
      </button>

      <!-- Option 2: Toggle Pin on Top -->
      <button
        type="button"
        class="w-full flex items-center justify-between px-3 py-2 text-left hover:bg-indigo-500/10 cursor-pointer transition-colors"
        @click="
          () => {
            closeMenus()
            store.togglePinOnTop()
          }
        "
      >
        <div class="flex items-center gap-2">
          <IconRenderer
            name="Pin"
            :size="14"
            :class-name="store.isPinnedOnTop ? 'text-indigo-400' : 'text-slate-400'"
          />
          <span>{{ store.isPinnedOnTop ? '取消窗口置顶' : '固定窗口在最前端' }}</span>
        </div>
        <span v-if="store.isPinnedOnTop" class="text-[10px] text-indigo-400 font-semibold">
          已置顶
        </span>
      </button>

      <!-- Option 3: Maximize / Restore -->
      <button
        type="button"
        class="w-full flex items-center justify-between px-3 py-2 text-left hover:bg-indigo-500/10 cursor-pointer transition-colors"
        @click="
          () => {
            closeMenus()
            store.toggleWindowMaximize()
          }
        "
      >
        <div class="flex items-center gap-2">
          <IconRenderer
            :name="store.isWindowMaximized ? 'Minimize2' : 'Maximize2'"
            :size="14"
            class-name="text-slate-400"
          />
          <span>{{ store.isWindowMaximized ? '还原默认窗口尺寸' : '放大窗口尺寸' }}</span>
        </div>
      </button>

      <!-- Option 4: Toggle Theme -->
      <button
        type="button"
        class="w-full flex items-center justify-between px-3 py-2 text-left hover:bg-indigo-500/10 cursor-pointer transition-colors"
        @click="
          () => {
            closeMenus()
            store.toggleTheme()
          }
        "
      >
        <div class="flex items-center gap-2">
          <IconRenderer
            :name="store.isDark ? 'Sun' : 'Moon'"
            :size="14"
            class-name="text-slate-400"
          />
          <span>{{ store.isDark ? '切换为浅色外观' : '切换为深色外观' }}</span>
        </div>
      </button>

      <div
        class="my-1 border-t"
        :class="store.isDark ? 'border-slate-800' : 'border-slate-100'"
      ></div>

      <!-- Option 5: Exit Plugin or Close Standalone Window -->
      <button
        type="button"
        class="w-full flex items-center justify-between px-3 py-2 text-left text-rose-500 hover:bg-rose-500/10 cursor-pointer transition-colors"
        @click="
          () => {
            closeMenus()
            if (store.isStandaloneWindow) {
              store.closeCurrentWindow()
            } else {
              store.closePlugin()
            }
          }
        "
      >
        <div class="flex items-center gap-2">
          <IconRenderer name="X" :size="14" />
          <span>{{ store.isStandaloneWindow ? '关闭独立窗口' : '退出当前工具' }}</span>
        </div>
        <kbd
          class="px-1.5 py-0.5 text-[10px] rounded font-mono"
          :class="store.isDark ? 'bg-slate-800 text-slate-400' : 'bg-slate-100 text-slate-500'"
        >
          {{ store.isStandaloneWindow ? '⌘W' : 'Esc' }}
        </kbd>
      </button>
    </div>
  </header>
</template>
