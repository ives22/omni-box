<script setup lang="ts">
import { ref, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import { CORE_PLUGIN_ITEMS, isTauriEnv } from '@/bridge/backend'
import { invoke } from '@tauri-apps/api/core'
import SearchBar from '@/components/SearchBar.vue'
import HomeGridDeck from '@/components/HomeGridDeck.vue'
import ResultList from '@/components/ResultList.vue'
import IconRenderer from '@/components/IconRenderer.vue'

// Built-in Fast Micro-App Plugins
import CalcPad from '@/plugins/calc-pad/CalcPad.vue'
import JsonStudio from '@/plugins/json-studio/JsonStudio.vue'
import DiffViewer from '@/plugins/diff-viewer/DiffViewer.vue'
import Authenticator from '@/plugins/authenticator/Authenticator.vue'
import PasswordVault from '@/plugins/password-vault/PasswordVault.vue'
import IpStudio from '@/plugins/ip-studio/IpStudio.vue'
import UnitConverter from '@/plugins/unit-converter/UnitConverter.vue'
import UtoolsHub from '@/plugins/utools-hub/UtoolsHub.vue'
import AiStudio from '@/plugins/ai-studio/AiStudio.vue'
import UserCenter from '@/plugins/user-center/UserCenter.vue'

interface WindowShownContext {
  hiddenMsAgo?: number
  copiedMsAgo?: number
  clipboardText?: string
}

const store = useLauncherStore()
const inTauri = isTauriEnv()
const isRecentGridExpanded = ref(false)
const contentRef = ref<HTMLElement | null>(null)
const searchBarRef = ref<InstanceType<typeof SearchBar> | null>(null)
let lastEscapeHandledAt = 0

// Synchronize native window height while keeping width (860px) and top-left position unchanged
function syncCompactLauncherHeight(isInitial = false) {
  if (!inTauri || store.isStandaloneWindow || store.activePluginId) return
  nextTick(() => {
    const targetWidth = store.isWindowMaximized ? 1120 : 860
    const showRecent = store.userSettings.showRecentInSearch !== false
    const isListMode = store.userSettings.searchBarMode === '列表模式'
    const baseIdleHeight = showRecent ? (isListMode ? 336 : 172) : 54
    let targetHeight = baseIdleHeight

    if (store.isSuperPanelOpen || store.aiAnswerState) {
      targetHeight = Math.max(baseIdleHeight, 370)
    } else if (store.searchQuery.trim()) {
      const count = Math.max(1, Math.min(8, store.results.length))
      targetHeight = store.results.length === 0 ? 188 : Math.min(580, 54 + count * 48 + 16)
    } else if (isRecentGridExpanded.value && showRecent) {
      // Match User Center / Settings height (580px) when "Recently Used" is expanded
      targetHeight = store.isWindowMaximized ? 740 : 580
    } else {
      targetHeight = baseIdleHeight
    }

    invoke('resize_window', {
      width: targetWidth,
      height: targetHeight,
      resizable: store.isDetachedWindow || isRecentGridExpanded.value,
      initPosition: isInitial,
      maximized: store.isWindowMaximized && isRecentGridExpanded.value
    })
      .then(() => {
        if (!store.activePluginId) {
          searchBarRef.value?.focusActiveInput()
        }
      })
      .catch(() => {})
  })
}

// Transitioning into/out of settings or plugins:
// Keep exact same width (860px) and top-left position by default;
// When double-clicked / maximized, expand to 86% x 84% of current display and restore on second double-click!
watch(
  [() => store.activePluginId, () => store.isWindowMaximized],
  ([newPluginId, isMax]) => {
    if (!inTauri) return
    if (store.isStandaloneWindow) {
      invoke('resize_window', {
        width: 1040,
        height: 680,
        resizable: true,
        initPosition: false,
        maximized: isMax
      }).catch(() => {})
      return
    }
    if (newPluginId) {
      invoke('resize_window', {
        width: 860,
        height: 580,
        resizable: true,
        initPosition: false,
        maximized: isMax
      }).catch(() => {})
    } else {
      syncCompactLauncherHeight(false)
    }
  }
)

watch(
  [
    () => store.searchQuery,
    () => store.results.length,
    isRecentGridExpanded,
    () => store.userSettings.showRecentInSearch,
    () => store.userSettings.searchBarMode,
    () => store.isSuperPanelOpen,
    () => store.aiAnswerState
  ],
  () => {
    if (!store.activePluginId) {
      syncCompactLauncherHeight(false)
    }
  }
)

function resetLauncherToCleanState() {
  store.searchQuery = ''
  store.subInputQuery = ''
  store.activePluginId = null
  store.isDetachedWindow = false
  store.isWindowMaximized = false
  store.isSuperPanelOpen = false
  store.aiAnswerState = null
  store.toastMessage = null
  isRecentGridExpanded.value = false
}

function resolveAutoClearThresholdMs(policy?: string): number {
  if (policy === '1 分钟') return 60_000
  if (policy === '3 分钟') return 180_000
  if (policy === '5 分钟') return 300_000
  return 0 // '立即清空'
}

function resolveAutoPasteWindowMs(policy?: string): number {
  if (policy === '3 秒内') return 3_000
  if (policy === '5 秒内') return 5_000
  if (policy === '10 秒内') return 10_000
  return 0 // '不自动粘贴'
}

function syncWindowSizeWithCurrentState(isInitial = false) {
  if (!inTauri) return
  if (store.activePluginId) {
    invoke('resize_window', {
      width: store.isStandaloneWindow ? 1040 : 860,
      height: store.isStandaloneWindow ? 680 : 580,
      resizable: true,
      initPosition: false,
      maximized: store.isWindowMaximized
    }).catch(() => {})
  } else {
    syncCompactLauncherHeight(isInitial)
  }
}

function handleWindowShownWithContext(ctx?: WindowShownContext) {
  store.isSuperPanelOpen = false
  store.aiAnswerState = null
  store.toastMessage = null

  const hiddenMsAgo = ctx?.hiddenMsAgo ?? 999_999_999
  const copiedMsAgo = ctx?.copiedMsAgo ?? 999_999_999
  const clipText = (ctx?.clipboardText || '').trim()

  // Active workbench tool continuity:
  // If the user was inside a tool (e.g. DiffViewer, JsonStudio, CalcPad, Authenticator, PasswordVault)
  // and did NOT press Esc to return to the search bar before switching apps / hiding the window,
  // keep them inside that tool if re-summoned within 5 minutes (300,000 ms)!
  const TOOL_CONTINUITY_TIMEOUT_MS = 300_000
  if (store.activePluginId) {
    const isConfigPage = store.activePluginId === 'user-center'
    if (!isConfigPage && hiddenMsAgo < TOOL_CONTINUITY_TIMEOUT_MS) {
      syncWindowSizeWithCurrentState(false)
      return
    }
    // Otherwise (left for >= 5 minutes or was in user-center settings), return to main search bar
    store.activePluginId = null
    store.subInputQuery = ''
    store.isDetachedWindow = false
    store.isWindowMaximized = false
  }

  isRecentGridExpanded.value = false

  const clearThresholdMs = resolveAutoClearThresholdMs(store.userSettings.autoClearWindow)
  const shouldClear = clearThresholdMs === 0 || hiddenMsAgo >= clearThresholdMs

  if (shouldClear) {
    store.searchQuery = ''
  }

  // Check Auto-Paste Window ("3 秒内" / "5 秒内" / "10 秒内")
  const pasteWindowMs = resolveAutoPasteWindowMs(store.userSettings.autoPasteWindow)
  if (pasteWindowMs > 0 && copiedMsAgo <= pasteWindowMs && clipText && clipText.length <= 500) {
    store.searchQuery = clipText
    syncWindowSizeWithCurrentState(false)
    searchBarRef.value?.selectActiveInput()
    return
  }

  syncWindowSizeWithCurrentState(false)

  if (!shouldClear && store.searchQuery.trim()) {
    searchBarRef.value?.selectActiveInput()
  } else {
    searchBarRef.value?.focusActiveInput()
  }
}

function triggerEscapeAction() {
  const now = Date.now()
  if (now - lastEscapeHandledAt < 180) return
  lastEscapeHandledAt = now

  const pluginEsc = (
    window as unknown as { __omniboxPluginInterceptEscape?: () => boolean }
  ).__omniboxPluginInterceptEscape
  if (pluginEsc && pluginEsc()) {
    return
  }

  // Standalone detached tool window NEVER closes or exits on Escape (only clears subInput if present)
  if (store.isStandaloneWindow) {
    if (store.subInputQuery) {
      store.subInputQuery = ''
    }
    return
  }

  if (store.isSuperPanelOpen) {
    store.isSuperPanelOpen = false
    return
  }
  if (store.aiAnswerState) {
    store.aiAnswerState = null
    return
  }
  if (store.activePluginId) {
    if (store.subInputQuery) {
      store.subInputQuery = ''
    } else {
      store.closePlugin()
    }
  } else {
    if (store.userSettings.autoClearWindow === '立即清空') {
      resetLauncherToCleanState()
    } else {
      store.activePluginId = null
      store.isDetachedWindow = false
      store.isWindowMaximized = false
      store.isSuperPanelOpen = false
      store.aiAnswerState = null
      isRecentGridExpanded.value = false
    }
    store.hideAppWindow()
  }
}

function handleGlobalKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    triggerEscapeAction()
    return
  }

  if ((e.metaKey || e.ctrlKey) && !e.shiftKey) {
    const map: Record<string, string> = {
      '1': 'calc-pad',
      '2': 'json-studio',
      '3': 'diff-viewer',
      '4': 'authenticator',
      '5': 'password-vault',
      '6': 'ip-studio',
      '7': 'unit-converter',
      '8': 'utools-hub',
      '9': 'ai-studio',
      ',': 'user-center'
    }
    const lowerKey = e.key.toLowerCase()
    const detachKey = (store.userSettings.detachWindowHotkey || 'Command+D')
      .split('+')
      .pop()
      ?.trim()
      .toLowerCase()

    if (map[e.key] && !store.isStandaloneWindow) {
      e.preventDefault()
      store.openPlugin(map[e.key])
    } else if (lowerKey === (detachKey || 'd')) {
      e.preventDefault()
      store.detachCurrentPluginToWindow()
    } else if (lowerKey === 'k' && !store.isStandaloneWindow) {
      e.preventDefault()
      store.closePlugin()
    } else if (lowerKey === 'w' || lowerKey === 'h') {
      e.preventDefault()
      if (store.isStandaloneWindow) {
        store.closeCurrentWindow()
      } else {
        resetLauncherToCleanState()
        store.hideAppWindow()
      }
    }
  }
}

function sendSuperPanelTextToPlugin(pluginId: string) {
  const text = store.superPanelClipboard.trim()
  store.isSuperPanelOpen = false
  if (pluginId === 'ai-translate') {
    store.openAiStudioWithAction('translate', text || 'Hello, welcome to OmniBox!')
    return
  }
  if (pluginId === 'ai-explain') {
    store.openAiStudioWithAction('explain', text || '请解释一下这段代码或文本的含义')
    return
  }
  if (pluginId === 'ai-summarize') {
    store.openAiStudioWithAction('summarize', text || '请帮我总结润色这段内容')
    return
  }
  if (pluginId === 'ai-ask') {
    store.triggerAiQuickAsk(text || '请介绍一下 OmniBox 的核心快捷功能')
    return
  }
  if (text) {
    store.searchQuery = text
  }
  store.openPlugin(pluginId)
}

function openAiSettingsFromQuickAsk() {
  ;(window as any).__omniboxUserCenterInitialTab = 'ai-models'
  store.aiAnswerState = null
  store.openPlugin('user-center')
}

async function copyTextWithToast(text: string, label = '已复制到剪贴板') {
  try {
    await navigator.clipboard.writeText(text)
    store.showToast(label)
  } catch {
    // ignore
  }
}

onMounted(async () => {
  const winObj = window as unknown as {
    __omniboxNativeEscape?: () => void
    __omniboxResetState?: () => void
    __omniboxOnWindowShown?: (ctx?: WindowShownContext) => void
    __omniboxOnWindowHidden?: () => void
    __omniboxOpenSuperPanel?: (payload?: { clipboardText?: string }) => void
    __omniboxOpenPlugin?: (pluginId: string) => void
    __omniboxOnNativeSettingChanged?: (patch: Record<string, unknown>) => void
    __omniboxOnTrayToast?: (msg: string) => void
  }
  winObj.__omniboxNativeEscape = triggerEscapeAction
  winObj.__omniboxResetState = resetLauncherToCleanState
  winObj.__omniboxOnWindowShown = handleWindowShownWithContext
  winObj.__omniboxOnWindowHidden = () => {
    store.isSuperPanelOpen = false
    store.aiAnswerState = null
    store.toastMessage = null
    // Note: Do NOT clear `store.activePluginId` on window hide so that if the user switches to
    // another app to copy text (e.g. Text 2 for DiffViewer) and re-opens OmniBox within 5 minutes,
    // the active tool and its live DOM / cursor state remain intact!
    if (!store.activePluginId) {
      isRecentGridExpanded.value = false
      if (store.userSettings.autoClearWindow === '立即清空') {
        store.searchQuery = ''
      }
    }
  }
  winObj.__omniboxOpenSuperPanel = (payload) => {
    store.openSuperPanel(payload?.clipboardText || '')
  }
  winObj.__omniboxOpenPlugin = (pluginId: string) => {
    store.isSuperPanelOpen = false
    store.aiAnswerState = null
    isRecentGridExpanded.value = false
    store.openPlugin(pluginId)
  }
  winObj.__omniboxOnNativeSettingChanged = (patch) => {
    store.userSettings = {
      ...store.userSettings,
      ...(patch as Partial<typeof store.userSettings>)
    }
  }
  winObj.__omniboxOnTrayToast = (msg: string) => {
    store.refreshSystemApps(false)
    store.showToast(msg)
  }

  await store.initStore()
  if (!store.isStandaloneWindow) {
    syncCompactLauncherHeight(true)
  }
  window.addEventListener('keydown', handleGlobalKeydown, true)
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKeydown, true)
})
</script>

<template>
  <div
    class="w-full flex flex-col items-center"
    :class="[
      inTauri
        ? 'w-screen h-screen bg-transparent p-0 m-0 justify-start overflow-hidden'
        : store.isDark
          ? 'min-h-screen bg-[#080B12] bg-[radial-gradient(ellipse_80%_50%_at_50%_-15%,rgba(99,102,241,0.14),transparent)] p-4 sm:pt-12 justify-start'
          : 'min-h-screen bg-slate-200/80 bg-[radial-gradient(ellipse_80%_50%_at_50%_-15%,rgba(79,70,229,0.10),transparent)] p-4 sm:pt-12 justify-start'
    ]"
  >
    <!-- Floating Toast Notification Pill (Inside plugins/settings or Super Panel) -->
    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="opacity-0 -translate-y-2 scale-95"
      enter-to-class="opacity-100 translate-y-0 scale-100"
      leave-active-class="transition duration-150 ease-in"
      leave-from-class="opacity-100 translate-y-0 scale-100"
      leave-to-class="opacity-0 -translate-y-2 scale-95"
    >
      <div
        v-if="store.toastMessage"
        role="status"
        aria-live="polite"
        class="fixed top-2.5 z-[120] pointer-events-none flex items-center gap-1.5 px-3.5 py-1.5 rounded-full text-xs font-medium shadow-xl border backdrop-blur-md"
        :class="
          store.isDark
            ? 'bg-[#0E131F]/95 border-emerald-500/40 text-slate-100'
            : 'bg-slate-900/95 text-white border-slate-700'
        "
      >
        <IconRenderer
          :name="store.toastType === 'warning' ? 'AlertTriangle' : 'Check'"
          :size="13"
          :class-name="store.toastType === 'warning' ? 'text-amber-400' : 'text-emerald-400'"
        />
        <span>{{ store.toastMessage }}</span>
      </div>
    </Transition>

    <!-- Main Container: 100% 1:1 Fill of Native Window in Tauri (Opacity handled natively via NSWindow setAlphaValue) -->
    <main
      class="flex flex-col rounded-[16px] overflow-hidden transition-colors duration-150"
      :class="[
        inTauri
          ? 'w-screen h-screen'
          : store.activePluginId || isRecentGridExpanded
            ? store.isWindowMaximized
              ? 'w-[86vw] max-w-[2200px] h-[84vh] shadow-launcher'
              : 'w-full max-w-[860px] h-[580px] shadow-launcher'
            : 'w-full max-w-[860px] shadow-launcher',
        store.isDark
          ? 'bg-[#0E131F] text-slate-100 border border-slate-700/80'
          : 'bg-[#F8FAFC] text-slate-900 border border-slate-300/90'
      ]"
    >
      <!-- Top 52px Header Bar -->
      <SearchBar ref="searchBarRef" />

      <!-- Content Area -->
      <div
        ref="contentRef"
        class="flex flex-col min-h-0 overflow-hidden"
        :class="
          store.activePluginId || isRecentGridExpanded || store.isSuperPanelOpen || store.aiAnswerState
            ? 'flex-1 h-full'
            : ''
        "
      >
        <template v-if="!store.activePluginId">
          <!-- Super Panel Overlay Card (Triggered by Long-Press Right/Middle Click or Test Button) -->
          <div
            v-if="store.isSuperPanelOpen"
            class="flex-1 overflow-y-auto px-5 py-3.5 space-y-3.5 select-none"
            :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
          >
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <span
                  class="inline-flex items-center justify-center w-6 h-6 rounded-lg bg-indigo-500/15 text-indigo-400"
                >
                  <IconRenderer name="Sparkles" :size="14" />
                </span>
                <span class="text-[13px] font-bold">超级面板 (Super Panel)</span>
                <span class="text-[11px] text-slate-400">
                  {{ store.userSettings.superPanelMouseTrigger }} · {{ store.userSettings.superPanelHoldMs }}ms
                </span>
              </div>
              <button
                type="button"
                class="text-xs text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 px-2 py-1 rounded cursor-pointer"
                @click="store.isSuperPanelOpen = false"
              >
                关闭 (Esc)
              </button>
            </div>

            <!-- Clipboard Context & Quick English Translation -->
            <div
              class="p-3 rounded-xl border space-y-2"
              :class="
                store.isDark
                  ? 'bg-slate-900/80 border-slate-800'
                  : 'bg-white border-slate-200/90 shadow-xs'
              "
            >
              <div class="flex items-center justify-between text-[11px] text-slate-400">
                <span>当前剪贴板 / 选中内容</span>
                <button
                  v-if="store.superPanelClipboard"
                  type="button"
                  class="text-indigo-400 hover:underline cursor-pointer"
                  @click="
                    () => {
                      store.searchQuery = store.superPanelClipboard
                      store.isSuperPanelOpen = false
                    }
                  "
                >
                  填入主搜索框
                </button>
              </div>
              <div
                class="text-xs font-mono line-clamp-2 break-all"
                :class="store.isDark ? 'text-slate-200' : 'text-slate-700'"
              >
                {{ store.superPanelClipboard || '（剪贴板暂无文本，复制任意单词/JSON/算式后呼出超级面板可自动识别）' }}
              </div>

              <!-- Quick English Translation Result -->
              <div
                v-if="store.superPanelTranslation"
                class="pt-2 border-t flex items-center justify-between gap-3"
                :class="store.isDark ? 'border-slate-800' : 'border-slate-100'"
              >
                <div class="flex items-center gap-2 min-w-0">
                  <span
                    class="px-1.5 py-0.5 text-[10px] font-semibold rounded bg-emerald-500/15 text-emerald-400 shrink-0"
                  >
                    英汉速译
                  </span>
                  <span class="text-xs font-semibold truncate text-emerald-400">
                    {{ store.superPanelTranslation }}
                  </span>
                </div>
                <button
                  type="button"
                  class="text-[11px] text-indigo-400 hover:underline shrink-0 cursor-pointer"
                  @click="copyTextWithToast(store.superPanelTranslation, '已复制翻译结果')"
                >
                  复制译文
                </button>
              </div>
            </div>

            <!-- Context Quick Actions -->
            <div class="grid grid-cols-3 gap-2">
              <button
                type="button"
                class="flex items-center gap-2 p-2.5 rounded-xl border text-xs font-medium cursor-pointer transition-colors"
                :class="
                  store.isDark
                    ? 'bg-slate-900/70 border-slate-800 hover:border-indigo-500/50 text-slate-200'
                    : 'bg-white border-slate-200/80 hover:border-indigo-400 text-slate-700'
                "
                @click="sendSuperPanelTextToPlugin('ai-translate')"
              >
                <IconRenderer name="Globe" :size="15" class-name="text-indigo-400" />
                <span>AI 深度翻译</span>
              </button>
              <button
                type="button"
                class="flex items-center gap-2 p-2.5 rounded-xl border text-xs font-medium cursor-pointer transition-colors"
                :class="
                  store.isDark
                    ? 'bg-slate-900/70 border-slate-800 hover:border-indigo-500/50 text-slate-200'
                    : 'bg-white border-slate-200/80 hover:border-indigo-400 text-slate-700'
                "
                @click="sendSuperPanelTextToPlugin('ai-explain')"
              >
                <IconRenderer name="Terminal" :size="15" class-name="text-violet-400" />
                <span>AI 解释代码</span>
              </button>
              <button
                type="button"
                class="flex items-center gap-2 p-2.5 rounded-xl border text-xs font-medium cursor-pointer transition-colors"
                :class="
                  store.isDark
                    ? 'bg-slate-900/70 border-slate-800 hover:border-indigo-500/50 text-slate-200'
                    : 'bg-white border-slate-200/80 hover:border-indigo-400 text-slate-700'
                "
                @click="sendSuperPanelTextToPlugin('ai-summarize')"
              >
                <IconRenderer name="Sparkles" :size="15" class-name="text-fuchsia-400" />
                <span>AI 总结润色</span>
              </button>
              <button
                type="button"
                class="flex items-center gap-2 p-2.5 rounded-xl border text-xs font-medium cursor-pointer transition-colors"
                :class="
                  store.isDark
                    ? 'bg-slate-900/70 border-slate-800 hover:border-indigo-500/50 text-slate-200'
                    : 'bg-white border-slate-200/80 hover:border-indigo-400 text-slate-700'
                "
                @click="sendSuperPanelTextToPlugin('json-studio')"
              >
                <IconRenderer name="Braces" :size="15" class-name="text-emerald-400" />
                <span>JSON 格式化</span>
              </button>
              <button
                type="button"
                class="flex items-center gap-2 p-2.5 rounded-xl border text-xs font-medium cursor-pointer transition-colors"
                :class="
                  store.isDark
                    ? 'bg-slate-900/70 border-slate-800 hover:border-indigo-500/50 text-slate-200'
                    : 'bg-white border-slate-200/80 hover:border-indigo-400 text-slate-700'
                "
                @click="sendSuperPanelTextToPlugin('calc-pad')"
              >
                <IconRenderer name="Calculator" :size="15" class-name="text-amber-400" />
                <span>送入计算稿纸</span>
              </button>
              <button
                type="button"
                class="flex items-center gap-2 p-2.5 rounded-xl border text-xs font-medium cursor-pointer transition-colors"
                :class="
                  store.isDark
                    ? 'bg-slate-900/70 border-slate-800 hover:border-indigo-500/50 text-slate-200'
                    : 'bg-white border-slate-200/80 hover:border-indigo-400 text-slate-700'
                "
                @click="sendSuperPanelTextToPlugin('diff-viewer')"
              >
                <IconRenderer name="GitCompare" :size="15" class-name="text-sky-400" />
                <span>文本对比</span>
              </button>
            </div>

            <!-- Pinned Core Plugins in Super Panel -->
            <div class="grid grid-cols-6 gap-2 pt-1">
              <button
                v-for="plug in CORE_PLUGIN_ITEMS.slice(0, 6)"
                :key="plug.id"
                type="button"
                class="flex flex-col items-center justify-center p-2 rounded-xl border text-[11px] cursor-pointer transition-colors"
                :class="
                  store.isDark
                    ? 'bg-slate-900/40 border-slate-800/80 hover:bg-slate-800/70 text-slate-300'
                    : 'bg-white border-slate-200/70 hover:bg-slate-100 text-slate-700'
                "
                @click="
                  () => {
                    store.isSuperPanelOpen = false
                    store.openPlugin(plug.action_payload)
                  }
                "
              >
                <IconRenderer :name="plug.icon" :size="16" class-name="text-indigo-400 mb-1" />
                <span class="truncate w-full text-center">{{ plug.title.split(' ')[0] }}</span>
              </button>
            </div>
          </div>

          <!-- AI Quick Answer Overlay Card -->
          <div
            v-else-if="store.aiAnswerState"
            class="flex-1 overflow-y-auto px-5 py-4 space-y-3"
            :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
          >
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-2 min-w-0">
                <IconRenderer name="Sparkles" :size="15" class-name="text-indigo-400 shrink-0" />
                <span class="text-xs font-bold truncate">
                  {{ store.userSettings.aiProvider }} · {{ store.userSettings.aiModelName }}
                </span>
                <span
                  v-if="store.aiAnswerState.latencyMs"
                  class="text-[10.5px] font-mono px-1.5 py-0.5 rounded"
                  :class="
                    store.isDark
                      ? 'bg-slate-800 text-slate-400'
                      : 'bg-slate-200/70 text-slate-600'
                  "
                >
                  {{ store.aiAnswerState.latencyMs }}ms
                </span>
              </div>
              <div class="flex items-center gap-2.5 shrink-0">
                <button
                  v-if="!store.aiAnswerState.error"
                  type="button"
                  class="px-2.5 py-1 rounded-lg text-[11.5px] font-semibold flex items-center gap-1 cursor-pointer transition-colors"
                  :class="
                    store.isDark
                      ? 'bg-indigo-500/20 text-indigo-300 hover:bg-indigo-500/30 border border-indigo-500/30'
                      : 'bg-indigo-50 text-indigo-600 hover:bg-indigo-100 border border-indigo-200'
                  "
                  @click="store.continueQuickAskInAiStudio()"
                >
                  <IconRenderer name="CornerUpRight" :size="12" />
                  <span>{{ store.aiAnswerState.loading ? '转入 AI 工作台 (⌘D)' : '继续追问 (AI 工作台)' }}</span>
                </button>
                <button
                  v-if="store.aiAnswerState.error"
                  type="button"
                  class="px-2.5 py-1 rounded-lg text-[11.5px] font-semibold flex items-center gap-1 cursor-pointer transition-colors"
                  :class="
                    store.isDark
                      ? 'bg-amber-500/20 text-amber-300 hover:bg-amber-500/30 border border-amber-500/30'
                      : 'bg-amber-50 text-amber-700 hover:bg-amber-100 border border-amber-200'
                  "
                  @click="openAiSettingsFromQuickAsk"
                >
                  <IconRenderer name="Settings" :size="12" />
                  <span>前往 AI 模型设置</span>
                </button>
                <button
                  v-if="!store.aiAnswerState.loading"
                  type="button"
                  class="text-xs text-slate-400 hover:text-indigo-400 cursor-pointer flex items-center gap-1"
                  @click="store.triggerAiQuickAsk(store.aiAnswerState.question)"
                >
                  <IconRenderer name="RefreshCw" :size="12" />
                  <span>重试</span>
                </button>
                <button
                  v-if="store.aiAnswerState.answer"
                  type="button"
                  class="text-xs text-indigo-400 hover:underline cursor-pointer"
                  @click="copyTextWithToast(store.aiAnswerState.answer, 'AI 回答已复制')"
                >
                  复制回答
                </button>
                <button
                  type="button"
                  class="text-xs text-slate-400 hover:text-slate-200 cursor-pointer"
                  @click="store.aiAnswerState = null"
                >
                  返回搜索 (Esc)
                </button>
              </div>
            </div>

            <!-- Reasoning Block if returned by DeepSeek R1 / reasoning models -->
            <details
              v-if="store.aiAnswerState.reasoningContent"
              class="rounded-xl border px-3 py-2 text-[11.5px]"
              :class="
                store.isDark
                  ? 'bg-slate-900/50 border-slate-800 text-slate-400'
                  : 'bg-slate-100/70 border-slate-200/80 text-slate-600'
              "
            >
              <summary class="cursor-pointer font-medium select-none">
                深度思考过程 (点击展开)
              </summary>
              <div class="mt-1.5 pt-1.5 border-t border-slate-700/30 whitespace-pre-wrap font-mono text-[11px] leading-relaxed">
                {{ store.aiAnswerState.reasoningContent }}
              </div>
            </details>

            <div
              class="p-3.5 rounded-xl border text-xs leading-relaxed whitespace-pre-wrap select-text"
              :class="
                store.aiAnswerState.error
                  ? store.isDark
                    ? 'bg-rose-950/30 border-rose-500/30 text-rose-200'
                    : 'bg-rose-50 border-rose-200 text-rose-800'
                  : store.isDark
                    ? 'bg-slate-900/90 border-slate-800 text-slate-200'
                    : 'bg-white border-slate-200 text-slate-800'
              "
            >
              <div v-if="store.aiAnswerState.loading" class="flex items-center gap-2 text-slate-400 py-4">
                <IconRenderer name="RefreshCw" :size="14" class-name="animate-spin text-indigo-400" />
                <span>正在通过原生通道请求 {{ store.userSettings.aiModelName }} 思考回答中...</span>
              </div>
              <div v-else>{{ store.aiAnswerState.answer }}</div>
            </div>
          </div>

          <template v-else-if="!store.searchQuery.trim()">
            <HomeGridDeck
              v-if="store.userSettings.showRecentInSearch !== false"
              @expand-change="(val) => (isRecentGridExpanded = val)"
            />
          </template>
          <ResultList v-else />
        </template>
        <CalcPad v-else-if="store.activePluginId === 'calc-pad'" />
        <JsonStudio v-else-if="store.activePluginId === 'json-studio'" />
        <DiffViewer v-else-if="store.activePluginId === 'diff-viewer'" />
        <Authenticator v-else-if="store.activePluginId === 'authenticator'" />
        <PasswordVault v-else-if="store.activePluginId === 'password-vault'" />
        <IpStudio v-else-if="store.activePluginId === 'ip-studio'" />
        <UnitConverter v-else-if="store.activePluginId === 'unit-converter'" />
        <UtoolsHub
          v-else-if="
            store.activePluginId === 'utools-hub' || store.activePluginId === 'utools-runner'
          "
        />
        <AiStudio v-else-if="store.activePluginId === 'ai-studio'" />
        <UserCenter v-else-if="store.activePluginId === 'user-center'" />
      </div>
    </main>
  </div>
</template>
