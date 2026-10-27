<script setup lang="ts">
import { ref, computed } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import {
  CORE_PLUGIN_ITEMS,
  SYSTEM_COMMAND_ITEMS,
  decryptVaultData,
  encryptVaultData,
  kvGet,
  kvPut,
  revealPathInFinder
} from '@/bridge/backend'
import IconRenderer from '@/components/IconRenderer.vue'

type TabKey =
  | 'account'
  | 'data'
  | 'security'
  | 'preferences'
  | 'ai-models'
  | 'local-launch'
  | 'shortcuts'
  | 'all-features'

const store = useLauncherStore()
// Default to 'preferences' (基本设置) or dynamic initial tab requested by plugins
const initialTabFromPlugin = (window as any).__omniboxUserCenterInitialTab as TabKey | undefined
if (initialTabFromPlugin) {
  delete (window as any).__omniboxUserCenterInitialTab
}
const activeTab = ref<TabKey>(initialTabFromPlugin || 'preferences')
const isRescanning = ref(false)

// Grouped Navigation Sidebar (uTools Reference Style: Short 4-char labels + Grouped sections)
const navGroups: {
  groupTitle: string
  items: { key: TabKey; label: string; icon: string; badgeCount?: boolean }[]
}[] = [
  {
    groupTitle: '个人中心',
    items: [
      { key: 'account', label: '我的账号', icon: 'UserCircle' },
      { key: 'data', label: '我的数据', icon: 'Database' },
      { key: 'security', label: '安全策略', icon: 'ShieldCheck' }
    ]
  },
  {
    groupTitle: '偏好设置',
    items: [
      { key: 'preferences', label: '基本设置', icon: 'Settings' },
      { key: 'ai-models', label: 'AI 模型', icon: 'Sparkles' },
      { key: 'local-launch', label: '本地启动', icon: 'Rocket', badgeCount: true },
      { key: 'shortcuts', label: '快捷方式', icon: 'CornerUpRight' },
      { key: 'all-features', label: '所有功能', icon: 'LayoutGrid' }
    ]
  }
]

// Alias Form State
const newAliasKey = ref('')
const newAliasTarget = ref('plugin:calc-pad')

// Security Master Password Rotation & Vault Policy State
const oldMasterPwd = ref('')
const newMasterPwd = ref('')
const showOldMasterPwd = ref(false)
const showNewMasterPwd = ref(false)
const vaultSessionUnlocked = ref(Boolean((window as any).__omniboxVaultSession))

// Backup Snapshot State
const snapshotJsonText = ref('')
const showImportBox = ref(false)

// All targetable items for custom aliases
const aliasTargetOptions = computed(() => {
  const list: { id: string; label: string }[] = []
  for (const p of CORE_PLUGIN_ITEMS) {
    list.push({ id: p.id, label: `[内置插件] ${p.title}` })
  }
  for (const app of store.systemApps) {
    list.push({ id: app.id, label: `[本机应用] ${app.title}` })
  }
  return list
})

const totalExecutions = computed(() => {
  return Object.values(store.usageStatsMap).reduce((acc, v) => acc + v, 0)
})

function resolveFriendlyUsageMeta(
  id: string,
  allMap: Map<string, { title: string; category: string }>
): { title: string; category: string } {
  const mapped = allMap.get(id)
  if (mapped) return mapped
  if (id === 'quick:math') {
    return { title: '快捷算式计算 (Quick Math)', category: '快捷指令' }
  }
  if (id === 'quick:json') {
    return { title: 'JSON 剪贴板速览 (Quick JSON)', category: '快捷指令' }
  }
  if (id.startsWith('totp-direct:')) {
    const raw = id.replace('totp-direct:totp-', '').replace('totp-direct:', '')
    const entry = store.totpEntries.find((e) => e.id.includes(raw))
    return {
      title: `2FA 动态码 · ${entry ? entry.issuer : raw.toUpperCase()}`,
      category: '2FA 直达'
    }
  }
  if (id.startsWith('sys:')) {
    return { title: `系统指令 (${id.replace('sys:', '')})`, category: '系统命令' }
  }
  return { title: id, category: '快捷指令' }
}

const topUsedList = computed(() => {
  const allMap = new Map<string, { title: string; category: string }>()
  for (const p of CORE_PLUGIN_ITEMS) {
    allMap.set(p.id, { title: p.title, category: '核心插件' })
  }
  for (const a of store.systemApps) {
    allMap.set(a.id, { title: a.title, category: '本机应用' })
  }

  const entries = Object.entries(store.usageStatsMap)
    .map(([id, count]) => {
      const meta = resolveFriendlyUsageMeta(id, allMap)
      return {
        id,
        count,
        title: meta.title,
        category: meta.category
      }
    })
    .sort((a, b) => b.count - a.count)
    .slice(0, 6)

  const maxCount = entries[0]?.count || 1
  return entries.map((e) => ({
    ...e,
    percent: Math.round((e.count / maxCount) * 100)
  }))
})

// Filtered local macOS applications inside Local Launch tab
const filteredSystemApps = computed(() => {
  const q = store.subInputQuery.trim().toLowerCase()
  if (!q) return store.systemApps
  return store.systemApps.filter(
    (app) =>
      app.title.toLowerCase().includes(q) ||
      app.subtitle.toLowerCase().includes(q) ||
      app.keywords.some((k) => k.toLowerCase().includes(q))
  )
})

const isRecordingGlobalHotkey = ref(false)
const isRecordingDetachHotkey = ref(false)

async function setGlobalHotkeyQuick(hk: string) {
  isRecordingGlobalHotkey.value = false
  await store.saveUserSettings({ globalHotkey: hk })
  store.showToast(`已切换搜索框快捷键为 ${hk}`)
}

function formatKeyEventToHotkey(e: KeyboardEvent): string | null {
  const ignoreKeys = ['Meta', 'Control', 'Alt', 'Shift', 'CapsLock']
  if (ignoreKeys.includes(e.key)) return null

  const parts: string[] = []
  if (e.metaKey) parts.push('Command')
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Option')
  if (e.shiftKey) parts.push('Shift')

  let keyToken = e.key
  if (e.code === 'Space' || keyToken === ' ') {
    keyToken = 'Space'
  } else if (e.code.startsWith('Key')) {
    keyToken = e.code.replace('Key', '').toUpperCase()
  } else if (e.code.startsWith('Digit')) {
    keyToken = e.code.replace('Digit', '')
  } else {
    keyToken = keyToken.length === 1 ? keyToken.toUpperCase() : keyToken
  }

  if (parts.length === 0) {
    parts.push('Option')
  }
  parts.push(keyToken)
  return parts.join(' + ')
}

async function handleRecordGlobalHotkey(e: KeyboardEvent) {
  e.preventDefault()
  e.stopPropagation()
  if (e.key === 'Escape') {
    isRecordingGlobalHotkey.value = false
    return
  }
  const formatted = formatKeyEventToHotkey(e)
  if (!formatted) return
  isRecordingGlobalHotkey.value = false
  await store.saveUserSettings({ globalHotkey: formatted })
  store.showToast(`已录制并启用快捷键: ${formatted}`)
}

async function handleRecordDetachHotkey(e: KeyboardEvent) {
  e.preventDefault()
  e.stopPropagation()
  if (e.key === 'Escape') {
    isRecordingDetachHotkey.value = false
    return
  }
  const formatted = formatKeyEventToHotkey(e)
  if (!formatted) return
  isRecordingDetachHotkey.value = false
  await store.saveUserSettings({ detachWindowHotkey: formatted.replace(/\s+/g, '') })
  store.showToast(`已更新分离独立窗口快捷键: ${formatted.replace(/\s+/g, '')}`)
}

async function testSuperPanelNow() {
  let clip = ''
  try {
    clip = await navigator.clipboard.readText()
  } catch {
    clip = 'Hello OmniBox Super Panel'
  }
  await store.openSuperPanel(clip || 'Hello OmniBox Super Panel')
}

async function handleThemeSelectChange(e: Event) {
  const val = (e.target as HTMLSelectElement).value as 'system' | 'dark' | 'light'
  store.setThemeMode(val)
}

async function addAlias() {
  const key = newAliasKey.value.trim().toLowerCase()
  if (!key) {
    store.showToast('请输入简短别名（如 wx / vc / cc）', 'warning')
    return
  }
  const updated = {
    ...store.userSettings.aliases,
    [key]: newAliasTarget.value
  }
  await store.saveUserSettings({ aliases: updated })
  newAliasKey.value = ''
  store.showToast(`已绑定快捷方式 "${key}"`)
}

async function removeAlias(key: string) {
  const copy = { ...store.userSettings.aliases }
  delete copy[key]
  await store.saveUserSettings({ aliases: copy })
  store.showToast(`已移除快捷方式 "${key}"`, 'info')
}

function getTargetTitle(id: string): string {
  const found = aliasTargetOptions.value.find((o) => o.id === id)
  return found ? found.label : id
}

async function toggleAppDisabled(appId: string) {
  const set = new Set(store.userSettings.disabledAppIds || [])
  if (set.has(appId)) {
    set.delete(appId)
    store.showToast('已恢复该应用在搜索框中的展示')
  } else {
    set.add(appId)
    store.showToast('已在搜索结果中隐藏该应用', 'info')
  }
  await store.saveUserSettings({ disabledAppIds: Array.from(set) })
}

async function handleRescanApps(clearCache = false) {
  isRescanning.value = true
  try {
    await store.refreshSystemApps(clearCache)
    store.showToast(
      `本机应用索引重建完成，共发现 ${store.systemApps.length} 款 macOS 应用`
    )
  } finally {
    isRescanning.value = false
  }
}

async function handleRotateMasterPassword() {
  if (!oldMasterPwd.value.trim() || !newMasterPwd.value.trim()) {
    store.showToast('请填写当前主密码与新主密码', 'warning')
    return
  }
  const cipherBlob = await kvGet<string | null>('password-vault', 'cipher_blob', null)
  if (!cipherBlob) {
    store.showToast('密码本尚未初始化，请先进入密码本解锁一次', 'warning')
    return
  }
  try {
    const nextKey = newMasterPwd.value.trim()
    const plainJson = await decryptVaultData(oldMasterPwd.value.trim(), cipherBlob)
    const reEncrypted = await encryptVaultData(nextKey, plainJson)
    await kvPut('password-vault', 'cipher_blob', reEncrypted)
    const globalObj = window as any
    if (globalObj.__omniboxVaultSession) {
      globalObj.__omniboxVaultSession.masterKey = nextKey
      globalObj.__omniboxVaultSession.unlockedAt = Date.now()
    }
    oldMasterPwd.value = ''
    newMasterPwd.value = ''
    showOldMasterPwd.value = false
    showNewMasterPwd.value = false
    store.showToast('主密码已更新，并使用 Argon2id + AES-256-GCM 重新加密全部条目')
  } catch {
    store.showToast('原主密码校验失败，无法更改', 'warning')
  }
}

function handleVaultAutoLockChange() {
  if (store.userSettings.vaultAutoLockMinutes === 0) {
    ;(window as any).__omniboxVaultSession = null
    vaultSessionUnlocked.value = false
  }
  store.saveUserSettings()
}

function forceLockVaultNow() {
  ;(window as any).__omniboxVaultSession = null
  vaultSessionUnlocked.value = false
  store.showToast('已擦除内存中的密码本密钥并立即锁定')
}

async function exportFullSnapshot() {
  const calcSheets = await kvGet('calc-pad', 'sheets', [])
  const vaultBlob = await kvGet('password-vault', 'cipher_blob', null)
  const snapshot = {
    app: 'OmniBox',
    version: '2.0.0',
    exportedAt: new Date().toISOString(),
    userSettings: store.userSettings,
    totpEntries: store.totpEntries,
    externalPlugins: store.externalPlugins,
    calcSheets,
    vaultBlob
  }
  const str = JSON.stringify(snapshot, null, 2)
  snapshotJsonText.value = str
  await navigator.clipboard.writeText(str)
  store.showToast('全量加密快照已生成并复制到剪贴板')
}

async function importFullSnapshot() {
  try {
    const parsed = JSON.parse(snapshotJsonText.value)
    if (parsed.userSettings) {
      await store.saveUserSettings(parsed.userSettings)
    }
    if (Array.isArray(parsed.totpEntries)) {
      await store.saveTotpEntries(parsed.totpEntries)
    }
    if (Array.isArray(parsed.externalPlugins)) {
      await store.saveExternalPlugins(parsed.externalPlugins)
    }
    if (Array.isArray(parsed.calcSheets)) {
      await kvPut('calc-pad', 'sheets', parsed.calcSheets)
    }
    if (parsed.vaultBlob) {
      await kvPut('password-vault', 'cipher_blob', parsed.vaultBlob)
    }
    showImportBox.value = false
    store.showToast('全量备份快照已成功恢复！')
  } catch {
    store.showToast('快照 JSON 格式有误，恢复失败', 'warning')
  }
}

async function triggerWebdavSync() {
  if (!store.userSettings.webdavUrl.trim()) {
    store.showToast('请先填写 WebDAV 服务器地址', 'warning')
    return
  }
  await exportFullSnapshot()
  await store.saveUserSettings({ lastSyncAt: Date.now() })
  store.showToast('已将本地加密快照同步至 WebDAV 云端端点')
}
</script>

<template>
  <div
    class="flex flex-1 h-full min-h-[480px] overflow-hidden select-none"
    :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
  >
    <!-- ========================================================================= -->
    <!-- Left Grouped Navigation Sidebar (uTools Style: Concise Labels + Market Entry) -->
    <!-- ========================================================================= -->
    <aside
      class="w-[180px] border-r flex flex-col justify-between px-3 py-3.5 shrink-0"
      :class="
        store.isDark
          ? 'bg-[#0A0E17] border-slate-800/90'
          : 'bg-[#F3F4F6]/90 border-slate-200/90'
      "
    >
      <div class="space-y-4 overflow-y-auto pr-0.5">
        <div
          v-for="group in navGroups"
          :key="group.groupTitle"
          class="space-y-1"
        >
          <div
            class="px-2.5 pb-1 text-[11px] font-semibold tracking-wide"
            :class="store.isDark ? 'text-slate-500' : 'text-slate-400'"
          >
            {{ group.groupTitle }}
          </div>

          <button
            v-for="tab in group.items"
            :key="tab.key"
            type="button"
            class="w-full flex items-center justify-between px-3 py-2 rounded-lg text-[13px] cursor-pointer transition-all duration-150"
            :class="
              activeTab === tab.key
                ? store.isDark
                  ? 'bg-indigo-500/15 text-indigo-300 font-semibold'
                  : 'bg-[#E2E4E9] text-slate-900 font-semibold'
                : store.isDark
                  ? 'text-slate-400 hover:bg-slate-800/50 hover:text-slate-200 font-normal'
                  : 'text-slate-700 hover:bg-slate-200/60 hover:text-slate-900 font-normal'
            "
            @click="activeTab = tab.key"
          >
            <div class="flex items-center gap-2.5 truncate">
              <IconRenderer
                :name="tab.icon"
                :size="15"
                :class-name="
                  activeTab === tab.key
                    ? store.isDark
                      ? 'text-indigo-400'
                      : 'text-slate-800'
                    : 'opacity-70'
                "
              />
              <span class="truncate">{{ tab.label }}</span>
            </div>

            <span
              v-if="tab.badgeCount"
              class="px-1.5 py-0.2 text-[10px] font-mono rounded-full"
              :class="
                store.isDark
                  ? 'bg-slate-800 text-slate-400'
                  : 'bg-slate-300/70 text-slate-600'
              "
            >
              {{ store.systemApps.length }}
            </span>
          </button>
        </div>
      </div>

      <!-- Bottom Fixed Button: Plugin Marketplace (Matches uTools Bottom Left Entry) -->
      <div class="pt-2 border-t" :class="store.isDark ? 'border-slate-800/80' : 'border-slate-200/80'">
        <button
          type="button"
          class="w-full flex items-center justify-center gap-2 py-2 px-3 rounded-lg text-xs font-medium cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'text-slate-300 hover:bg-slate-800/70 hover:text-white'
              : 'text-slate-700 hover:bg-slate-200/75 hover:text-slate-950'
          "
          @click="store.openPlugin('utools-hub')"
        >
          <IconRenderer name="Blocks" :size="15" class-name="text-indigo-500" />
          <span>插件应用市场</span>
        </button>
      </div>
    </aside>

    <!-- ========================================================================= -->
    <!-- Right Content Area -->
    <!-- ========================================================================= -->
    <div
      class="flex-1 overflow-y-auto px-7 py-5"
      :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
    >
      <!-- ======================================================================= -->
      <!-- TAB: 基本设置 (Preferences - Modeled after uTools Settings Screenshots) -->
      <!-- ======================================================================= -->
      <template v-if="activeTab === 'preferences'">
        <div class="max-w-[600px] space-y-7 pb-4">
          <!-- Section 1: 使用偏好 -->
          <section class="space-y-4">
            <h3
              class="text-[13px] font-semibold"
              :class="store.isDark ? 'text-indigo-400' : 'text-[#4E5BA6]'"
            >
              使用偏好
            </h3>

            <div class="space-y-4 pl-3">
              <!-- 搜索框快捷键 (支持点击直接录制任意组合键 + 快捷预设) -->
              <div class="flex items-start justify-between gap-4">
                <div class="pt-1.5 text-[13px] font-medium">搜索框快捷键</div>
                <div class="w-[260px] flex flex-col items-end">
                  <button
                    type="button"
                    aria-label="点击录制搜索框快捷键"
                    class="w-full h-[34px] px-3 rounded-md border text-center font-mono text-[13px] font-medium outline-none cursor-pointer transition-all"
                    :class="
                      isRecordingGlobalHotkey
                        ? 'bg-indigo-500/20 border-indigo-500 text-indigo-400 ring-2 ring-indigo-500/30 animate-pulse'
                        : store.isDark
                          ? 'bg-slate-900/90 border-slate-700 text-indigo-300 hover:border-indigo-500'
                          : 'bg-white border-slate-300/90 text-[#3B4CCA] hover:border-indigo-500'
                    "
                    @click="isRecordingGlobalHotkey = !isRecordingGlobalHotkey"
                    @keydown="isRecordingGlobalHotkey ? handleRecordGlobalHotkey($event) : undefined"
                    @blur="isRecordingGlobalHotkey = false"
                  >
                    {{
                      isRecordingGlobalHotkey
                        ? '请按下组合键 (Esc 取消)...'
                        : store.userSettings.globalHotkey
                    }}
                  </button>
                  <div class="flex items-center gap-3 mt-1.5 text-[11px] font-mono">
                    <button
                      type="button"
                      class="cursor-pointer hover:underline"
                      :class="store.isDark ? 'text-indigo-400' : 'text-[#3B4CCA]'"
                      @click="setGlobalHotkeyQuick('Option + Space')"
                    >
                      ⌥+Space
                    </button>
                    <button
                      type="button"
                      class="cursor-pointer hover:underline"
                      :class="store.isDark ? 'text-indigo-400' : 'text-[#3B4CCA]'"
                      @click="setGlobalHotkeyQuick('Command + Space')"
                    >
                      ⌘+Space
                    </button>
                    <button
                      type="button"
                      class="cursor-pointer hover:underline"
                      :class="store.isDark ? 'text-indigo-400' : 'text-[#3B4CCA]'"
                      @click="setGlobalHotkeyQuick('Double Command')"
                    >
                      双击 ⌘
                    </button>
                  </div>
                </div>
              </div>

              <!-- 失去焦点隐藏窗口 -->
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-1.5 text-[13px] font-medium">
                  <span>点击外部自动隐藏窗口</span>
                  <span
                    title="当鼠标点击搜索框窗口外部区域时，自动收起并隐藏搜索窗口"
                    class="text-slate-400 cursor-help"
                  >
                    <IconRenderer name="HelpCircle" :size="13" />
                  </span>
                </div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.hideOnBlur"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>

              <!-- 剪贴板智能感知 -->
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-1.5 text-[13px] font-medium">
                  <span>剪贴板 JSON / 算式智能感知</span>
                  <span
                    title="当输入或粘贴合法的 JSON 字符串或数学算式时，自动在第一行提供直达处理建议"
                    class="text-slate-400 cursor-help"
                  >
                    <IconRenderer name="HelpCircle" :size="13" />
                  </span>
                </div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.smartClipboard"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>
            </div>
          </section>

          <!-- Section 2: 超级面板 -->
          <section class="space-y-4">
            <h3
              class="text-[13px] font-semibold"
              :class="store.isDark ? 'text-indigo-400' : 'text-[#4E5BA6]'"
            >
              超级面板
            </h3>

            <div class="space-y-4 pl-3">
              <!-- 启用超级面板 -->
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">启用超级面板</div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.enableSuperPanel"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>

              <!-- 鼠标按键弹出 -->
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">鼠标按键弹出</div>
                <select
                  v-model="store.userSettings.superPanelMouseTrigger"
                  aria-label="鼠标按键弹出方式"
                  class="w-[260px] h-[32px] px-3 rounded-md border text-[12.5px] outline-none cursor-pointer"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200'
                      : 'bg-white border-slate-300/90 text-slate-800'
                  "
                  @change="store.saveUserSettings()"
                >
                  <option value="长按鼠标右键">长按鼠标右键</option>
                  <option value="长按鼠标中键">长按鼠标中键</option>
                  <option value="Ctrl + 鼠标右键">Ctrl + 鼠标右键</option>
                </select>
              </div>

              <!-- 长按毫秒响应 -->
              <div class="flex items-start justify-between">
                <div class="text-[13px] font-medium pt-1">长按毫秒响应</div>
                <div class="w-[260px]">
                  <input
                    v-model.number="store.userSettings.superPanelHoldMs"
                    type="range"
                    min="250"
                    max="350"
                    step="50"
                    aria-label="长按毫秒响应"
                    class="w-full accent-[#4E5BA6] dark:accent-indigo-500 cursor-pointer"
                    @change="store.saveUserSettings()"
                  />
                  <div class="flex justify-between text-[11px] text-slate-400 px-1 mt-0.5">
                    <span>250 毫秒</span>
                    <span>300 毫秒</span>
                    <span>350 毫秒</span>
                  </div>
                </div>
              </div>

              <!-- 英文快速翻译 -->
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-1.5 text-[13px] font-medium">
                  <span>英文快速翻译</span>
                  <span
                    title="选中英文单词或段落呼出超级面板时，自动展示中文释义"
                    class="text-slate-400 cursor-help"
                  >
                    <IconRenderer name="HelpCircle" :size="13" />
                  </span>
                </div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.quickTranslate"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>

              <!-- 自动固定“功能” -->
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-1.5 text-[13px] font-medium">
                  <span>自动固定“功能”</span>
                  <span
                    title="根据使用频率自动将高频使用的插件固定在面板首屏"
                    class="text-slate-400 cursor-help"
                  >
                    <IconRenderer name="HelpCircle" :size="13" />
                  </span>
                </div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.autoPinFeature"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>

              <!-- 立即测试呼出超级面板 -->
              <div class="flex items-center justify-between pt-1">
                <div class="text-[12px] text-slate-400">
                  在任意窗口{{ store.userSettings.superPanelMouseTrigger }}即可呼出超级面板
                </div>
                <button
                  type="button"
                  class="px-3 py-1.5 rounded-lg text-xs font-medium border cursor-pointer transition-colors"
                  :class="
                    store.isDark
                      ? 'bg-indigo-500/15 border-indigo-500/30 text-indigo-300 hover:bg-indigo-500/25'
                      : 'bg-indigo-50 border-indigo-200 text-indigo-600 hover:bg-indigo-100'
                  "
                  @click="testSuperPanelNow"
                >
                  立即测试呼出超级面板
                </button>
              </div>
            </div>
          </section>

          <!-- Section 3: 主题样式 -->
          <section class="space-y-4">
            <h3
              class="text-[13px] font-semibold"
              :class="store.isDark ? 'text-indigo-400' : 'text-[#4E5BA6]'"
            >
              主题样式
            </h3>

            <div class="space-y-4 pl-3">
              <!-- 主题模式 -->
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">主题</div>
                <select
                  :value="store.userSettings.themeMode || (store.isDark ? 'dark' : 'light')"
                  aria-label="选择主题外观"
                  class="w-[260px] h-[32px] px-3 rounded-md border text-[12.5px] outline-none cursor-pointer"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200'
                      : 'bg-white border-slate-300/90 text-slate-800'
                  "
                  @change="handleThemeSelectChange"
                >
                  <option value="system">跟随系统</option>
                  <option value="light">极简浅色 (Light)</option>
                  <option value="dark">深邃曜石 (Dark)</option>
                </select>
              </div>

              <!-- 搜索框窗口不透明度 -->
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">
                  搜索框窗口不透明度 ({{ store.userSettings.windowOpacity ?? 98 }}%)
                </div>
                <div class="w-[260px]">
                  <input
                    v-model.number="store.userSettings.windowOpacity"
                    type="range"
                    min="75"
                    max="100"
                    step="1"
                    aria-label="搜索框窗口不透明度"
                    class="w-full accent-[#4E5BA6] dark:accent-indigo-500 cursor-pointer"
                    @input="store.saveUserSettings()"
                  />
                </div>
              </div>

              <!-- 搜索框占位提示语 -->
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">搜索框占位提示词</div>
                <input
                  v-model="store.userSettings.searchPlaceholder"
                  type="text"
                  placeholder="搜索功能 / 本机应用 / 粘贴文本或输入算式"
                  aria-label="搜索框占位提示词"
                  class="w-[260px] h-[32px] px-3 rounded-md border text-[12px] outline-none"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200 focus:border-indigo-500'
                      : 'bg-white border-slate-300/90 text-slate-800 focus:border-indigo-500'
                  "
                  @change="store.saveUserSettings()"
                />
              </div>
            </div>
          </section>

          <!-- Section 4: 高级设置 -->
          <section class="space-y-4">
            <h3
              class="text-[13px] font-semibold"
              :class="store.isDark ? 'text-indigo-400' : 'text-[#4E5BA6]'"
            >
              高级设置
            </h3>

            <div class="space-y-4 pl-3">
              <!-- 开机启动 -->
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">开机启动</div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.launchAtLogin"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>

              <!-- 搜索框显示「最近使用」 -->
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">搜索框显示「最近使用」</div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.showRecentInSearch"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>

              <!-- 搜索框匹配显示「应用推荐」 -->
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">搜索框匹配显示「应用推荐」</div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.showAppRecommendation"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>

              <!-- 分离为独立窗口快捷键 -->
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-1.5 text-[13px] font-medium">
                  <span>分离为独立窗口快捷键</span>
                  <span
                    title="点击右侧按钮可自定义快捷键；按下即可将当前窗口切换为独立 macOS 窗口"
                    class="text-slate-400 cursor-help"
                  >
                    <IconRenderer name="HelpCircle" :size="13" />
                  </span>
                </div>
                <button
                  type="button"
                  aria-label="点击录制分离为独立窗口快捷键"
                  class="w-[260px] h-[32px] flex items-center justify-center rounded-md border font-mono text-[13px] font-medium cursor-pointer transition-all"
                  :class="
                    isRecordingDetachHotkey
                      ? 'bg-indigo-500/20 border-indigo-500 text-indigo-400 ring-2 ring-indigo-500/30 animate-pulse'
                      : store.isDark
                        ? 'bg-slate-900 border-slate-700 text-indigo-300 hover:border-indigo-500'
                        : 'bg-white border-slate-300/90 text-[#3B4CCA] hover:border-indigo-500'
                  "
                  @click="isRecordingDetachHotkey = !isRecordingDetachHotkey"
                  @keydown="isRecordingDetachHotkey ? handleRecordDetachHotkey($event) : undefined"
                  @blur="isRecordingDetachHotkey = false"
                >
                  {{
                    isRecordingDetachHotkey
                      ? '请按下组合键 (Esc 取消)...'
                      : store.userSettings.detachWindowHotkey || 'Command+D'
                  }}
                </button>
              </div>

              <!-- 搜索框模式 -->
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">搜索框模式</div>
                <select
                  v-model="store.userSettings.searchBarMode"
                  aria-label="搜索框模式"
                  class="w-[260px] h-[32px] px-3 rounded-md border text-[12.5px] outline-none cursor-pointer"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200'
                      : 'bg-white border-slate-300/90 text-slate-800'
                  "
                  @change="store.saveUserSettings()"
                >
                  <option value="聚合模式">聚合模式</option>
                  <option value="列表模式">列表模式</option>
                </select>
              </div>

              <!-- 自动粘贴搜索框 -->
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-1.5 text-[13px] font-medium">
                  <span>自动粘贴搜索框</span>
                  <span
                    title="复制文本后在指定时间内呼出搜索框，自动将剪贴板内容填入或感知"
                    class="text-slate-400 cursor-help"
                  >
                    <IconRenderer name="HelpCircle" :size="13" />
                  </span>
                </div>
                <select
                  v-model="store.userSettings.autoPasteWindow"
                  aria-label="自动粘贴搜索框时间窗口"
                  class="w-[260px] h-[32px] px-3 rounded-md border text-[12.5px] outline-none cursor-pointer"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200'
                      : 'bg-white border-slate-300/90 text-slate-800'
                  "
                  @change="store.saveUserSettings()"
                >
                  <option value="不自动粘贴">不自动粘贴</option>
                  <option value="3 秒内">3 秒内</option>
                  <option value="5 秒内">5 秒内</option>
                  <option value="10 秒内">10 秒内</option>
                </select>
              </div>

              <!-- 自动清空搜索框 -->
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-1.5 text-[13px] font-medium">
                  <span>自动清空搜索框</span>
                  <span
                    title="每次重新通过快捷键呼出工具窗口时，自动清空上一次的搜索关键词"
                    class="text-slate-400 cursor-help"
                  >
                    <IconRenderer name="HelpCircle" :size="13" />
                  </span>
                </div>
                <select
                  v-model="store.userSettings.autoClearWindow"
                  aria-label="自动清空搜索框策略"
                  class="w-[260px] h-[32px] px-3 rounded-md border text-[12.5px] outline-none cursor-pointer"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200'
                      : 'bg-white border-slate-300/90 text-slate-800'
                  "
                  @change="store.saveUserSettings()"
                >
                  <option value="立即清空">每次呼出立即清空</option>
                  <option value="1 分钟">1 分钟</option>
                  <option value="3 分钟">3 分钟</option>
                  <option value="5 分钟">5 分钟</option>
                </select>
              </div>

              <!-- 空格键执行打开 -->
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-1.5 text-[13px] font-medium">
                  <span>空格键执行打开</span>
                  <span
                    title="开启后，在搜索结果列表中按空格键也可直接打开当前高亮的应用或插件"
                    class="text-slate-400 cursor-help"
                  >
                    <IconRenderer name="HelpCircle" :size="13" />
                  </span>
                </div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.spaceToOpen"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>
            </div>
          </section>

          <!-- Section 5: 网络代理 -->
          <section class="space-y-4">
            <h3
              class="text-[13px] font-semibold"
              :class="store.isDark ? 'text-indigo-400' : 'text-[#4E5BA6]'"
            >
              网络代理
            </h3>

            <div class="space-y-4 pl-3">
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">使用 HTTP / SOCKS5 网络代理</div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.proxyEnabled"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>

              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">代理服务器地址</div>
                <input
                  v-model="store.userSettings.proxyHost"
                  type="text"
                  placeholder="http://127.0.0.1:7890"
                  aria-label="代理服务器地址"
                  class="w-[260px] h-[32px] px-3 rounded-md border font-mono text-[12px] outline-none"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200 focus:border-indigo-500'
                      : 'bg-white border-slate-300/90 text-slate-800 focus:border-indigo-500'
                  "
                  @change="store.saveUserSettings()"
                />
              </div>
            </div>
          </section>
        </div>
      </template>

      <!-- ======================================================================= -->
      <!-- TAB: 我的账号 (Account & Efficiency Dashboard) -->
      <!-- ======================================================================= -->
      <template v-else-if="activeTab === 'account'">
        <div class="space-y-4">
          <div
            class="flex items-center justify-between p-4 rounded-2xl border"
            :class="
              store.isDark
                ? 'bg-obsidian-800/60 border-slate-800/90'
                : 'bg-white border-slate-200/80 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
            "
          >
            <div class="flex items-center gap-3.5 min-w-0">
              <div
                class="flex items-center justify-center w-11 h-11 rounded-2xl bg-gradient-to-br from-indigo-500 to-violet-600 text-white font-mono text-base font-bold shadow-sm shadow-indigo-500/20 shrink-0"
              >
                OB
              </div>
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <input
                    v-model="store.userSettings.nickname"
                    type="text"
                    aria-label="个人昵称"
                    class="bg-transparent text-[15px] font-bold outline-none focus:ring-2 focus:ring-indigo-500/30 rounded px-1 -ml-1"
                    :class="store.isDark ? 'text-white' : 'text-slate-900'"
                    @change="store.saveUserSettings()"
                  />
                  <span
                    class="px-2 py-0.5 text-[10px] font-mono font-semibold rounded-full border shrink-0"
                    :class="
                      store.isDark
                        ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
                        : 'bg-indigo-50 text-indigo-600 border-indigo-200/70'
                    "
                  >
                    UNLIMITED PRO
                  </span>
                </div>
                <p class="text-xs text-slate-400 mt-0.5 truncate">{{ store.userSettings.roleTitle }}</p>
              </div>
            </div>

            <div class="text-right shrink-0 ml-4">
              <div class="text-[11px] text-slate-400 mb-1">当前呼出快捷键</div>
              <span
                class="inline-flex items-center px-2.5 py-1 rounded-lg font-mono text-xs font-medium border whitespace-nowrap"
                :class="
                  store.isDark
                    ? 'bg-slate-800/90 text-indigo-300 border-slate-700'
                    : 'bg-slate-100 text-slate-700 border-slate-200/90'
                "
              >
                {{ store.userSettings.globalHotkey }}
              </span>
            </div>
          </div>

          <!-- 4 Metric Cards -->
          <div class="grid grid-cols-4 gap-3">
            <div
              v-for="stat in [
                { label: '已索引本机 App', value: store.systemApps.length, unit: '款应用' },
                { label: '已激活插件总数', value: store.externalPlugins.length + 6, unit: '无上限' },
                { label: '平均检索延迟', value: store.searchLatencyMs, unit: 'ms' },
                { label: '累计指令直达', value: totalExecutions, unit: '次执行' }
              ]"
              :key="stat.label"
              class="p-3.5 rounded-2xl border"
              :class="
                store.isDark
                  ? 'bg-obsidian-950/60 border-slate-800/90'
                  : 'bg-white border-slate-200/80 shadow-[0_1px_2px_rgba(15,23,42,0.02)]'
              "
            >
              <div class="text-[11px] text-slate-400">{{ stat.label }}</div>
              <div class="mt-1.5 flex items-baseline gap-1.5 font-mono">
                <span
                  class="text-xl font-bold tracking-tight"
                  :class="store.isDark ? 'text-white' : 'text-slate-900'"
                >
                  {{ stat.value }}
                </span>
                <span class="text-[10px] text-slate-400">{{ stat.unit }}</span>
              </div>
            </div>
          </div>

          <!-- Top Frecency Ranking -->
          <div
            class="p-4 rounded-2xl border space-y-3"
            :class="
              store.isDark
                ? 'bg-obsidian-950/40 border-slate-800/80'
                : 'bg-white border-slate-200/80 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
            "
          >
            <div class="flex items-center justify-between text-xs">
              <span class="font-semibold" :class="store.isDark ? 'text-slate-100' : 'text-slate-800'">
                最常使用指令与本机应用 (Frecency 智能权重排行)
              </span>
              <span class="text-slate-400 text-[11px]">越常用排序越靠前</span>
            </div>

            <div v-if="topUsedList.length === 0" class="py-6 text-center text-xs text-slate-400">
              暂无使用统计，在主搜索框启动任意插件或本机应用后将自动记录频率排行
            </div>

            <div v-else class="space-y-2.5 pt-0.5">
              <div
                v-for="item in topUsedList"
                :key="item.id"
                class="space-y-1.5 text-xs"
              >
                <div class="flex items-center justify-between">
                  <div class="flex items-center gap-2 min-w-0">
                    <span
                      class="font-medium truncate"
                      :class="store.isDark ? 'text-slate-200' : 'text-slate-800'"
                    >
                      {{ item.title }}
                    </span>
                    <span
                      class="text-[10px] px-1.5 py-0.5 rounded-md font-normal shrink-0"
                      :class="
                        store.isDark
                          ? 'bg-slate-800 text-slate-400 border border-slate-700/70'
                          : 'bg-slate-100 text-slate-500 border border-slate-200/70'
                      "
                    >
                      {{ item.category }}
                    </span>
                  </div>
                  <span
                    class="font-mono text-[11px] font-medium shrink-0"
                    :class="store.isDark ? 'text-indigo-300' : 'text-indigo-600'"
                  >
                    {{ item.count }} 次
                  </span>
                </div>
                <div
                  class="w-full h-1.5 rounded-full overflow-hidden"
                  :class="store.isDark ? 'bg-slate-800/80' : 'bg-slate-100'"
                >
                  <div
                    class="h-full rounded-full bg-gradient-to-r from-indigo-500 to-violet-500 transition-all duration-300"
                    :style="{ width: `${item.percent}%` }"
                  ></div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </template>

      <!-- ======================================================================= -->
      <!-- TAB: 我的数据 (Data Storage, WebDAV Cloud Sync & JSON Snapshot) -->
      <!-- ======================================================================= -->
      <template v-else-if="activeTab === 'data'">
        <div class="space-y-4 text-xs">
          <div
            class="p-4 rounded-2xl border space-y-3"
            :class="
              store.isDark
                ? 'bg-obsidian-950/50 border-slate-800'
                : 'bg-white border-slate-200/80 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
            "
          >
            <div class="flex items-center justify-between gap-3">
              <div>
                <h3 class="font-bold text-[13px]">WebDAV 私有云多端同步</h3>
                <p class="text-[11px] text-slate-400 mt-0.5">
                  支持坚果云、Nextcloud、群晖 NAS、Alist 等任意 WebDAV 服务，同步前自动经 AES-256-GCM 加密
                </p>
              </div>
              <button
                type="button"
                class="btn-primary shrink-0"
                @click="triggerWebdavSync"
              >
                <IconRenderer name="RefreshCw" :size="13" />
                <span>立即同步至云端</span>
              </button>
            </div>

            <div class="grid grid-cols-3 gap-2.5">
              <input
                v-model="store.userSettings.webdavUrl"
                type="text"
                placeholder="WebDAV 地址 (如 https://dav.jianguoyun.com/dav/)"
                aria-label="WebDAV 服务器地址"
                class="px-3 py-1.5 rounded-xl font-mono border outline-none focus:ring-2 focus:ring-indigo-500/25 focus:border-indigo-500"
                :class="
                  store.isDark
                    ? 'bg-obsidian-950 border-slate-700 text-slate-200'
                    : 'bg-slate-50/70 border-slate-200/90 text-slate-800'
                "
                @change="store.saveUserSettings()"
              />
              <input
                v-model="store.userSettings.webdavUsername"
                type="text"
                placeholder="WebDAV 账号"
                aria-label="WebDAV 账号"
                class="px-3 py-1.5 rounded-xl font-mono border outline-none focus:ring-2 focus:ring-indigo-500/25 focus:border-indigo-500"
                :class="
                  store.isDark
                    ? 'bg-obsidian-950 border-slate-700 text-slate-200'
                    : 'bg-slate-50/70 border-slate-200/90 text-slate-800'
                "
                @change="store.saveUserSettings()"
              />
              <input
                v-model="store.userSettings.webdavPassword"
                type="password"
                placeholder="应用专用密码"
                aria-label="WebDAV 密码"
                class="px-3 py-1.5 rounded-xl font-mono border outline-none focus:ring-2 focus:ring-indigo-500/25 focus:border-indigo-500"
                :class="
                  store.isDark
                    ? 'bg-obsidian-950 border-slate-700 text-slate-200'
                    : 'bg-slate-50/70 border-slate-200/90 text-slate-800'
                "
                @change="store.saveUserSettings()"
              />
            </div>
          </div>

          <div
            class="p-4 rounded-2xl border space-y-3"
            :class="
              store.isDark
                ? 'bg-obsidian-950/50 border-slate-800'
                : 'bg-white border-slate-200/80 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
            "
          >
            <div class="flex items-center justify-between gap-3">
              <div>
                <h3 class="font-bold text-[13px]">本地全量快照备份与恢复</h3>
                <p class="text-[11px] text-slate-400 mt-0.5">
                  包含计算稿纸、2FA 令牌、密码本加密卷、uTools 插件配置及快捷方式别名
                </p>
              </div>
              <div class="flex items-center gap-2 shrink-0">
                <button
                  type="button"
                  class="btn-secondary"
                  @click="exportFullSnapshot"
                >
                  <IconRenderer name="Download" :size="13" />
                  <span>导出备份</span>
                </button>
                <button
                  type="button"
                  class="btn-secondary"
                  @click="showImportBox = !showImportBox"
                >
                  <IconRenderer name="Upload" :size="13" />
                  <span>导入恢复</span>
                </button>
              </div>
            </div>

            <textarea
              v-if="snapshotJsonText || showImportBox"
              v-model="snapshotJsonText"
              rows="6"
              spellcheck="false"
              placeholder="在此粘贴备份的 JSON 快照内容..."
              aria-label="全量备份 JSON 文本框"
              class="w-full p-3 rounded-xl font-mono text-[11px] border outline-none focus:ring-2 focus:ring-indigo-500/25"
              :class="
                store.isDark
                  ? 'bg-obsidian-950 border-slate-800 text-indigo-200'
                  : 'bg-slate-50 border-slate-200 text-slate-800'
              "
            ></textarea>

            <button
              v-if="showImportBox"
              type="button"
              class="btn-primary"
              @click="importFullSnapshot"
            >
              确认覆盖并恢复备份
            </button>
          </div>
        </div>
      </template>

      <!-- ======================================================================= -->
      <!-- TAB: 安全策略 (Security & Master Key Policy) -->
      <!-- ======================================================================= -->
      <template v-else-if="activeTab === 'security'">
        <div class="space-y-4 text-xs">
          <!-- Card 1: Master Password Rotation + Auto-Lock Policy -->
          <div
            class="p-4 rounded-2xl border space-y-3.5"
            :class="
              store.isDark
                ? 'bg-obsidian-950/50 border-slate-800'
                : 'bg-white border-slate-200/80 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
            "
          >
            <div class="flex items-center justify-between gap-2">
              <div>
                <h3 class="font-bold text-[13px]">密码本主密码轮换与自动锁定策略 (Argon2id + AES-256-GCM)</h3>
                <p class="text-[11px] text-slate-400 mt-0.5">
                  更改主密码时，系统将验证旧主密码并使用新的 Argon2id 盐值重新加密所有密码条目（初始默认密码为
                  <code
                    class="font-mono px-1.5 py-0.5 rounded"
                    :class="
                      store.isDark
                        ? 'bg-indigo-500/15 text-indigo-300'
                        : 'bg-indigo-50 text-indigo-600'
                    "
                  >
                    omnibox2026
                  </code>
                  ）
                </p>
              </div>
              <div class="flex items-center gap-2 shrink-0">
                <span
                  class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium border"
                  :class="
                    vaultSessionUnlocked
                      ? store.isDark
                        ? 'bg-emerald-500/15 border-emerald-500/30 text-emerald-400'
                        : 'bg-emerald-50 border-emerald-200 text-emerald-600'
                      : store.isDark
                        ? 'bg-slate-800 border-slate-700 text-slate-400'
                        : 'bg-slate-100 border-slate-200 text-slate-500'
                  "
                >
                  <IconRenderer :name="vaultSessionUnlocked ? 'Unlock' : 'Lock'" :size="11" />
                  <span>{{ vaultSessionUnlocked ? '当前已解锁' : '当前已锁定' }}</span>
                </span>
                <button
                  v-if="vaultSessionUnlocked"
                  type="button"
                  class="px-2 py-1 rounded-lg text-[11px] font-medium border transition-colors cursor-pointer"
                  :class="
                    store.isDark
                      ? 'bg-rose-500/15 border-rose-500/30 text-rose-300 hover:bg-rose-500/25'
                      : 'bg-rose-50 border-rose-200 text-rose-600 hover:bg-rose-100'
                  "
                  @click="forceLockVaultNow"
                >
                  立即锁定
                </button>
              </div>
            </div>

            <div class="grid grid-cols-3 gap-2.5">
              <!-- Current Master Password Input with Eye Toggle -->
              <div class="relative flex items-center">
                <input
                  v-model="oldMasterPwd"
                  :type="showOldMasterPwd ? 'text' : 'password'"
                  placeholder="当前主密码"
                  aria-label="当前主密码"
                  class="w-full pl-3 pr-8 py-1.5 rounded-xl font-mono border outline-none focus:ring-2 focus:ring-indigo-500/25 focus:border-indigo-500"
                  :class="
                    store.isDark
                      ? 'bg-obsidian-950 border-slate-700 text-white'
                      : 'bg-slate-50/70 border-slate-200/90 text-slate-800'
                  "
                />
                <button
                  type="button"
                  class="absolute right-2 p-1 rounded text-slate-400 hover:text-indigo-500 transition-colors cursor-pointer"
                  :title="showOldMasterPwd ? '隐藏当前主密码' : '查看当前主密码'"
                  @click="showOldMasterPwd = !showOldMasterPwd"
                >
                  <IconRenderer :name="showOldMasterPwd ? 'EyeOff' : 'Eye'" :size="14" />
                </button>
              </div>

              <!-- New Master Password Input with Eye Toggle -->
              <div class="relative flex items-center">
                <input
                  v-model="newMasterPwd"
                  :type="showNewMasterPwd ? 'text' : 'password'"
                  placeholder="新主密码"
                  aria-label="新主密码"
                  class="w-full pl-3 pr-8 py-1.5 rounded-xl font-mono border outline-none focus:ring-2 focus:ring-indigo-500/25 focus:border-indigo-500"
                  :class="
                    store.isDark
                      ? 'bg-obsidian-950 border-slate-700 text-white'
                      : 'bg-slate-50/70 border-slate-200/90 text-slate-800'
                  "
                />
                <button
                  type="button"
                  class="absolute right-2 p-1 rounded text-slate-400 hover:text-indigo-500 transition-colors cursor-pointer"
                  :title="showNewMasterPwd ? '隐藏新主密码' : '查看新主密码'"
                  @click="showNewMasterPwd = !showNewMasterPwd"
                >
                  <IconRenderer :name="showNewMasterPwd ? 'EyeOff' : 'Eye'" :size="14" />
                </button>
              </div>

              <button
                type="button"
                class="btn-primary"
                @click="handleRotateMasterPassword"
              >
                更新主密码
              </button>
            </div>

            <!-- Divider: Auto-Lock Duration & Clipboard Auto-Clear -->
            <div
              class="pt-3 border-t grid grid-cols-1 sm:grid-cols-2 gap-3"
              :class="store.isDark ? 'border-slate-800/80' : 'border-slate-100'"
            >
              <!-- Setting 1: Auto-lock duration after unlock -->
              <div
                class="p-3 rounded-xl border flex items-center justify-between gap-2"
                :class="
                  store.isDark
                    ? 'bg-obsidian-900/60 border-slate-800/90'
                    : 'bg-slate-50/70 border-slate-200/70'
                "
              >
                <div class="min-w-0">
                  <div class="font-semibold text-[12px] flex items-center gap-1.5">
                    <IconRenderer name="Clock" :size="13" class="text-indigo-500 shrink-0" />
                    <span>解锁后自动锁定时间</span>
                  </div>
                  <div class="text-[11px] text-slate-400 mt-0.5">
                    超时后自动擦除内存密钥并重新锁定
                  </div>
                </div>
                <select
                  v-model.number="store.userSettings.vaultAutoLockMinutes"
                  aria-label="密码本解锁后自动锁定时间"
                  class="px-2.5 py-1.5 rounded-lg text-xs font-medium border outline-none cursor-pointer shrink-0"
                  :class="
                    store.isDark
                      ? 'bg-obsidian-950 border-slate-700 text-slate-200'
                      : 'bg-white border-slate-200 text-slate-700 shadow-2xs'
                  "
                  @change="handleVaultAutoLockChange"
                >
                  <option :value="0">离开即刻锁定 (0分钟)</option>
                  <option :value="1">1 分钟</option>
                  <option :value="5">5 分钟</option>
                  <option :value="15">15 分钟 (默认推荐)</option>
                  <option :value="30">30 分钟</option>
                  <option :value="60">60 分钟 (1小时)</option>
                  <option :value="-1">应用运行期间保持解锁</option>
                </select>
              </div>

              <!-- Setting 2: Auto-clear clipboard after copying sensitive password/AK/SK -->
              <div
                class="p-3 rounded-xl border flex items-center justify-between gap-2"
                :class="
                  store.isDark
                    ? 'bg-obsidian-900/60 border-slate-800/90'
                    : 'bg-slate-50/70 border-slate-200/70'
                "
              >
                <div class="min-w-0">
                  <div class="font-semibold text-[12px] flex items-center gap-1.5">
                    <IconRenderer name="ClipboardCheck" :size="13" class="text-indigo-500 shrink-0" />
                    <span>复制密码后自动清理剪贴板</span>
                  </div>
                  <div class="text-[11px] text-slate-400 mt-0.5">
                    复制密码或 SK 后倒计时清空剪贴板防泄漏
                  </div>
                </div>
                <select
                  v-model.number="store.userSettings.vaultClipboardClearSeconds"
                  aria-label="复制敏感密码后自动清理剪贴板时间"
                  class="px-2.5 py-1.5 rounded-lg text-xs font-medium border outline-none cursor-pointer shrink-0"
                  :class="
                    store.isDark
                      ? 'bg-obsidian-950 border-slate-700 text-slate-200'
                      : 'bg-white border-slate-200 text-slate-700 shadow-2xs'
                  "
                  @change="store.saveUserSettings()"
                >
                  <option :value="0">不自动清理</option>
                  <option :value="15">15 秒后清空</option>
                  <option :value="30">30 秒后清空 (默认)</option>
                  <option :value="60">60 秒后清空</option>
                </select>
              </div>
            </div>
          </div>

          <div
            class="p-4 rounded-2xl border flex items-center justify-between"
            :class="
              store.isDark
                ? 'bg-obsidian-950/50 border-slate-800'
                : 'bg-white border-slate-200/80 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
            "
          >
            <div>
              <div class="font-bold text-[13px]">主搜索框直接展示 2FA 动态验证码</div>
              <div class="text-[11px] text-slate-400 mt-0.5">
                开启后在主搜索框输入平台名（如 github）可直接看到 6 位动态码并回车复制
              </div>
            </div>
            <label class="relative inline-flex items-center cursor-pointer">
              <input
                v-model="store.userSettings.directTotpInSearch"
                type="checkbox"
                class="sr-only peer"
                @change="store.saveUserSettings()"
              />
              <div
                class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
              ></div>
            </label>
          </div>
        </div>
      </template>

      <!-- ======================================================================= -->
      <!-- TAB: AI 模型 (AI Models & Agent Endpoint Configuration) -->
      <!-- ======================================================================= -->
      <template v-else-if="activeTab === 'ai-models'">
        <div class="max-w-[600px] space-y-6 pb-4">
          <section class="space-y-4">
            <h3
              class="text-[13px] font-semibold"
              :class="store.isDark ? 'text-indigo-400' : 'text-[#4E5BA6]'"
            >
              AI 模型与 Agent 连接
            </h3>

            <div class="space-y-4 pl-3">
              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">启用搜索框 AI 快捷问答</div>
                <label class="relative inline-flex items-center cursor-pointer">
                  <input
                    v-model="store.userSettings.aiQuickAskEnabled"
                    type="checkbox"
                    class="sr-only peer"
                    @change="store.saveUserSettings()"
                  />
                  <div
                    class="w-9 h-5 bg-slate-300 dark:bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-[#4E5BA6] dark:peer-checked:bg-indigo-500"
                  ></div>
                </label>
              </div>

              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">默认 AI 服务提供商</div>
                <select
                  v-model="store.userSettings.aiProvider"
                  aria-label="默认 AI 服务提供商"
                  class="w-[260px] h-[32px] px-3 rounded-md border text-[12.5px] outline-none cursor-pointer"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200'
                      : 'bg-white border-slate-300/90 text-slate-800'
                  "
                  @change="store.saveUserSettings()"
                >
                  <option value="Google Gemini">Google Gemini</option>
                  <option value="DeepSeek">DeepSeek (深度求索)</option>
                  <option value="OpenAI">OpenAI ChatGPT</option>
                  <option value="Anthropic Claude">Anthropic Claude</option>
                  <option value="Ollama 本地模型">Ollama 本地大模型</option>
                </select>
              </div>

              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">模型名称 (Model ID)</div>
                <input
                  v-model="store.userSettings.aiModelName"
                  type="text"
                  placeholder="gemini-2.5-pro / deepseek-chat"
                  aria-label="AI 模型名称"
                  class="w-[260px] h-[32px] px-3 rounded-md border font-mono text-[12px] outline-none"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200 focus:border-indigo-500'
                      : 'bg-white border-slate-300/90 text-slate-800 focus:border-indigo-500'
                  "
                  @change="store.saveUserSettings()"
                />
              </div>

              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">API Endpoint 地址</div>
                <input
                  v-model="store.userSettings.aiBaseUrl"
                  type="text"
                  placeholder="https://api.openai.com/v1"
                  aria-label="API Endpoint 地址"
                  class="w-[260px] h-[32px] px-3 rounded-md border font-mono text-[12px] outline-none"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200 focus:border-indigo-500'
                      : 'bg-white border-slate-300/90 text-slate-800 focus:border-indigo-500'
                  "
                  @change="store.saveUserSettings()"
                />
              </div>

              <div class="flex items-center justify-between">
                <div class="text-[13px] font-medium">API Key 密钥</div>
                <input
                  v-model="store.userSettings.aiApiKey"
                  type="password"
                  placeholder="sk-..."
                  aria-label="API Key 密钥"
                  class="w-[260px] h-[32px] px-3 rounded-md border font-mono text-[12px] outline-none"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200 focus:border-indigo-500'
                      : 'bg-white border-slate-300/90 text-slate-800 focus:border-indigo-500'
                  "
                  @change="store.saveUserSettings()"
                />
              </div>
            </div>
          </section>
        </div>
      </template>

      <!-- ======================================================================= -->
      <!-- TAB: 本地启动 (Local macOS Apps & Spotlight Indexer) -->
      <!-- ======================================================================= -->
      <template v-else-if="activeTab === 'local-launch'">
        <div class="space-y-3">
          <div
            class="flex items-center justify-between gap-4 p-4 rounded-2xl border text-xs"
            :class="
              store.isDark
                ? 'bg-obsidian-950/60 border-slate-800'
                : 'bg-white border-slate-200/80 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
            "
          >
            <div class="min-w-0">
              <div class="font-bold text-[13px]">
                macOS 本地启动索引库 (已索引 {{ store.systemApps.length }} 款应用)
              </div>
              <div class="text-[11px] text-slate-400 mt-1 leading-relaxed">
                自动扫描 <code class="font-mono">/Applications</code> 并读取 Spotlight 中文名称与高清图标；主搜索框输入
                <code
                  class="font-mono px-1 py-0.5 rounded"
                  :class="
                    store.isDark
                      ? 'bg-indigo-500/15 text-indigo-300'
                      : 'bg-indigo-50 text-indigo-600'
                  "
                >
                  f 关键词
                </code>
                可聚焦搜索本地文件
              </div>
            </div>

            <button
              type="button"
              :disabled="isRescanning"
              class="btn-primary shrink-0"
              @click="handleRescanApps(true)"
            >
              <IconRenderer
                name="RefreshCw"
                :size="13"
                :class-name="isRescanning ? 'animate-spin' : ''"
              />
              <span>{{ isRescanning ? '正在扫描...' : '重建索引与图标' }}</span>
            </button>
          </div>

          <div class="grid grid-cols-2 gap-2.5">
            <div
              v-for="app in filteredSystemApps"
              :key="app.id"
              class="flex items-center justify-between p-2.5 rounded-xl border text-xs transition-all duration-150"
              :class="[
                store.userSettings.disabledAppIds.includes(app.id) ? 'opacity-50' : '',
                store.isDark
                  ? 'bg-obsidian-900/70 border-slate-800/90 hover:border-slate-700'
                  : 'bg-white border-slate-200/80 hover:border-slate-300/90 shadow-[0_1px_2px_rgba(15,23,42,0.02)]'
              ]"
            >
              <div class="flex items-center gap-2.5 min-w-0 pr-2">
                <img
                  v-if="app.icon_data_url"
                  :src="app.icon_data_url"
                  :alt="app.title"
                  class="w-8 h-8 rounded-lg object-contain shrink-0"
                />
                <div
                  v-else
                  class="flex items-center justify-center w-8 h-8 rounded-lg shrink-0"
                  :class="
                    store.isDark
                      ? 'bg-slate-800 text-slate-300'
                      : 'bg-slate-100 text-slate-600'
                  "
                >
                  <IconRenderer name="AppWindow" :size="16" />
                </div>

                <div class="min-w-0">
                  <div
                    class="font-semibold truncate"
                    :class="store.isDark ? 'text-slate-100' : 'text-slate-800'"
                  >
                    {{ app.title }}
                  </div>
                  <div class="text-[10px] text-slate-400 font-mono truncate">{{ app.subtitle }}</div>
                </div>
              </div>

              <div class="flex items-center gap-1.5 shrink-0">
                <button
                  type="button"
                  title="在访达 (Finder) 中定位"
                  aria-label="在访达中定位应用"
                  class="p-1.5 rounded-lg text-slate-400 hover:text-slate-700 dark:hover:text-white hover:bg-slate-100 dark:hover:bg-slate-800 cursor-pointer transition-colors"
                  @click="revealPathInFinder(app.action_payload)"
                >
                  <IconRenderer name="FolderGit2" :size="13" />
                </button>
                <button
                  type="button"
                  :title="store.userSettings.disabledAppIds.includes(app.id) ? '点击恢复显示' : '点击在搜索结果中隐藏'"
                  class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-medium border cursor-pointer transition-all duration-150"
                  :class="
                    store.userSettings.disabledAppIds.includes(app.id)
                      ? store.isDark
                        ? 'bg-rose-500/15 border-rose-500/30 text-rose-300'
                        : 'bg-rose-50 border-rose-200 text-rose-600'
                      : store.isDark
                        ? 'bg-slate-800/70 border-slate-700/80 text-slate-300 hover:bg-slate-700'
                        : 'bg-slate-100/90 border-slate-200/80 text-slate-600 hover:bg-slate-200/70'
                  "
                  @click="toggleAppDisabled(app.id)"
                >
                  <span
                    class="w-1.5 h-1.5 rounded-full"
                    :class="
                      store.userSettings.disabledAppIds.includes(app.id)
                        ? 'bg-rose-500'
                        : 'bg-indigo-500'
                    "
                  ></span>
                  <span>{{ store.userSettings.disabledAppIds.includes(app.id) ? '已隐藏' : '显示中' }}</span>
                </button>
              </div>
            </div>
          </div>
        </div>
      </template>

      <!-- ======================================================================= -->
      <!-- TAB: 快捷方式 (Custom Keyword Aliases & Fast Triggers) -->
      <!-- ======================================================================= -->
      <template v-else-if="activeTab === 'shortcuts'">
        <div class="space-y-4">
          <div
            class="p-4 rounded-2xl border space-y-3"
            :class="
              store.isDark
                ? 'bg-obsidian-950/50 border-slate-800'
                : 'bg-white border-slate-200/80 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
            "
          >
            <div>
              <h3 class="text-[13px] font-bold">自定义搜索别名直达 (Custom Shortcuts)</h3>
              <p class="text-[11px] text-slate-400 mt-0.5">
                为常用插件或本机 App 设置简短缩写（如 <code class="font-mono">wx</code>、<code class="font-mono">cc</code>），在搜索框输入时直接置顶命中
              </p>
            </div>

            <div class="flex items-center gap-2 text-xs">
              <input
                v-model="newAliasKey"
                type="text"
                placeholder="输入简写别名 (如 wx / py / cc)"
                aria-label="自定义别名缩写"
                class="w-44 px-3 py-1.5 rounded-xl font-mono border outline-none focus:ring-2 focus:ring-indigo-500/25 focus:border-indigo-500"
                :class="
                  store.isDark
                    ? 'bg-obsidian-950 border-slate-700 text-indigo-300'
                    : 'bg-slate-50/70 border-slate-200/90 text-slate-800'
                "
              />
              <span class="text-slate-400">→</span>
              <select
                v-model="newAliasTarget"
                aria-label="绑定目标插件或本机应用"
                class="flex-1 px-3 py-1.5 rounded-xl border outline-none focus:ring-2 focus:ring-indigo-500/25 focus:border-indigo-500"
                :class="
                  store.isDark
                    ? 'bg-obsidian-950 border-slate-700 text-slate-200'
                    : 'bg-slate-50/70 border-slate-200/90 text-slate-800'
                "
              >
                <option v-for="opt in aliasTargetOptions" :key="opt.id" :value="opt.id">
                  {{ opt.label }}
                </option>
              </select>
              <button
                type="button"
                class="btn-primary shrink-0"
                @click="addAlias"
              >
                添加快捷方式
              </button>
            </div>

            <div class="grid grid-cols-2 gap-2.5 pt-1">
              <div
                v-for="(targetId, aliasKey) in store.userSettings.aliases"
                :key="aliasKey"
                class="flex items-center justify-between px-3 py-2 rounded-xl border text-xs"
                :class="
                  store.isDark
                    ? 'bg-obsidian-900 border-slate-800'
                    : 'bg-slate-50/80 border-slate-200/80'
                "
              >
                <div class="flex items-center gap-2 truncate pr-2">
                  <span
                    class="px-2 py-0.5 rounded-md font-mono font-semibold border"
                    :class="
                      store.isDark
                        ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
                        : 'bg-indigo-50 text-indigo-600 border-indigo-200/70'
                    "
                  >
                    {{ aliasKey }}
                  </span>
                  <span class="text-slate-400">→</span>
                  <span
                    class="truncate"
                    :class="store.isDark ? 'text-slate-300' : 'text-slate-700'"
                  >
                    {{ getTargetTitle(targetId) }}
                  </span>
                </div>
                <button
                  type="button"
                  aria-label="删除此快捷方式"
                  class="text-slate-400 hover:text-rose-500 cursor-pointer transition-colors"
                  @click="removeAlias(String(aliasKey))"
                >
                  <IconRenderer name="Trash2" :size="13" />
                </button>
              </div>
            </div>
          </div>
        </div>
      </template>

      <!-- ======================================================================= -->
      <!-- TAB: 所有功能 (All Built-in Plugins & System Commands Overview) -->
      <!-- ======================================================================= -->
      <template v-else-if="activeTab === 'all-features'">
        <div class="space-y-4 text-xs">
          <div class="space-y-2.5">
            <h3
              class="text-[13px] font-semibold"
              :class="store.isDark ? 'text-indigo-400' : 'text-[#4E5BA6]'"
            >
              内置极速插件 ({{ CORE_PLUGIN_ITEMS.length }})
            </h3>
            <div class="grid grid-cols-2 gap-2.5">
              <div
                v-for="plugin in CORE_PLUGIN_ITEMS"
                :key="plugin.id"
                class="flex items-center justify-between p-3 rounded-xl border cursor-pointer transition-all"
                :class="
                  store.isDark
                    ? 'bg-obsidian-900/70 border-slate-800 hover:border-indigo-500/50'
                    : 'bg-white border-slate-200/80 hover:border-indigo-300 shadow-[0_1px_2px_rgba(15,23,42,0.02)]'
                "
                @click="store.openPlugin(plugin.action_payload)"
              >
                <div class="flex items-center gap-2.5 min-w-0 pr-2">
                  <div
                    class="flex items-center justify-center w-8 h-8 rounded-lg shrink-0"
                    :class="
                      store.isDark
                        ? 'bg-indigo-500/15 text-indigo-300'
                        : 'bg-indigo-50 text-indigo-600'
                    "
                  >
                    <IconRenderer :name="plugin.icon" :size="16" />
                  </div>
                  <div class="min-w-0">
                    <div class="font-semibold truncate">{{ plugin.title }}</div>
                    <div class="text-[10px] text-slate-400 truncate mt-0.5">
                      指令词: {{ plugin.keywords.slice(0, 4).join(' / ') }}
                    </div>
                  </div>
                </div>
                <IconRenderer name="ChevronRight" :size="14" class-name="text-slate-400 shrink-0" />
              </div>
            </div>
          </div>

          <div class="space-y-2.5 pt-2">
            <h3
              class="text-[13px] font-semibold"
              :class="store.isDark ? 'text-indigo-400' : 'text-[#4E5BA6]'"
            >
              系统快捷指令 ({{ SYSTEM_COMMAND_ITEMS.length }})
            </h3>
            <div class="grid grid-cols-2 gap-2.5">
              <div
                v-for="cmd in SYSTEM_COMMAND_ITEMS"
                :key="cmd.id"
                class="flex items-center justify-between p-3 rounded-xl border"
                :class="
                  store.isDark
                    ? 'bg-obsidian-900/70 border-slate-800'
                    : 'bg-white border-slate-200/80 shadow-[0_1px_2px_rgba(15,23,42,0.02)]'
                "
              >
                <div class="flex items-center gap-2.5 min-w-0 pr-2">
                  <div
                    class="flex items-center justify-center w-8 h-8 rounded-lg shrink-0"
                    :class="
                      store.isDark
                        ? 'bg-slate-800 text-slate-300'
                        : 'bg-slate-100 text-slate-600'
                    "
                  >
                    <IconRenderer :name="cmd.icon" :size="16" />
                  </div>
                  <div class="min-w-0">
                    <div class="font-semibold truncate">{{ cmd.title }}</div>
                    <div class="text-[10px] text-slate-400 truncate mt-0.5">
                      指令词: {{ cmd.keywords.slice(0, 4).join(' / ') }}
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>
