<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import {
  TotpEntry,
  captureScreenRegionBase64,
  readClipboardImageBase64,
  isTauriEnv
} from '@/bridge/backend'
import IconRenderer from '@/components/IconRenderer.vue'
import {
  decodeQrFromImageSource,
  parseOtpPayload,
  buildOtpauthQrCode,
  inferCategoryFromIssuer,
  sanitizeBase32Secret
} from './qrEngine'

const store = useLauncherStore()

const showAddModal = ref(false)
const otpauthUriInput = ref('')
const formIssuer = ref('')
const formAccount = ref('')
const formSecret = ref('')
const formCategory = ref('开发平台')

const isScanningScreen = ref(false)
const isDraggingImage = ref(false)
const highlightedEntryId = ref<string | null>(null)
const fileInputRef = ref<HTMLInputElement | null>(null)

// Reverse QR Code Modal State (for syncing to mobile Authenticator)
const activeQrModalEntry = ref<TotpEntry | null>(null)
const activeQrDataUrl = ref('')
const activeQrOtpUri = ref('')
const showRawSecretInModal = ref(false)

const filteredEntries = computed(() => {
  const q = store.subInputQuery.trim().toLowerCase()
  if (!q) return store.totpEntries
  return store.totpEntries.filter(
    (item) =>
      item.issuer.toLowerCase().includes(q) ||
      item.account.toLowerCase().includes(q) ||
      item.category.toLowerCase().includes(q)
  )
})

function formatCode(code?: string): string {
  if (!code || code.length < 6) return '--- ---'
  return `${code.slice(0, 3)} ${code.slice(3)}`
}

function flashHighlightCard(entryId: string) {
  highlightedEntryId.value = entryId
  setTimeout(() => {
    if (highlightedEntryId.value === entryId) {
      highlightedEntryId.value = null
    }
  }, 2600)
}

async function copyCode(entry: TotpEntry) {
  const live = store.totpLiveMap[entry.id]
  if (!live) return
  await navigator.clipboard.writeText(live.code)
  store.showToast(`已复制 ${entry.issuer} 动态验证码：${live.code}`)
}

async function copyPlainText(text: string, message: string) {
  await navigator.clipboard.writeText(text)
  store.showToast(message)
}

/**
 * Mode A: Unified handler for recognized QR text / pasted otpauth URI.
 * - If complete (has issuer/account + secret): auto-saves immediately and copies current 6-digit code!
 * - If duplicate secret exists: highlights existing card and copies its current code!
 * - If only raw Base32 secret (no issuer): opens the Add Drawer with secret pre-filled.
 */
async function processRecognizedPayload(rawText: string, sourceTag = '扫码') {
  const parsed = parseOtpPayload(rawText)
  if (!parsed.ok) {
    store.showToast(parsed.error || `${sourceTag}未能解析出合法的 2FA 密钥`, 'warning')
    return
  }

  // 1. Check duplicate secret to prevent redundant cards
  const existing = store.totpEntries.find(
    (item) => sanitizeBase32Secret(item.secret) === parsed.secret
  )
  if (existing) {
    flashHighlightCard(existing.id)
    const liveCode = store.totpLiveMap[existing.id]?.code
    if (liveCode) {
      await navigator.clipboard.writeText(liveCode)
      store.showToast(
        `该密钥已存在 (${existing.issuer} · ${existing.account})，已直接复制验证码：${liveCode}`,
        'info'
      )
    } else {
      store.showToast(`该 2FA 密钥已存在：${existing.issuer} (${existing.account})`, 'info')
    }
    return
  }

  // 2. Mode A Direct Save: when both issuer and secret are present
  if (parsed.isComplete) {
    const newEntry: TotpEntry = {
      id: `totp-${Date.now()}`,
      issuer: parsed.issuer,
      account: parsed.account,
      secret: parsed.secret,
      period: parsed.period,
      digits: parsed.digits,
      category: parsed.category
    }

    await store.saveTotpEntries([newEntry, ...store.totpEntries])
    formIssuer.value = ''
    formAccount.value = ''
    formSecret.value = ''
    otpauthUriInput.value = ''
    showAddModal.value = false
    flashHighlightCard(newEntry.id)

    const liveCode = store.totpLiveMap[newEntry.id]?.code
    if (liveCode) {
      await navigator.clipboard.writeText(liveCode)
      store.showToast(
        `${sourceTag}添加成功：${newEntry.issuer} · 已自动复制验证码 ${liveCode}`
      )
    } else {
      store.showToast(`${sourceTag}添加成功：${newEntry.issuer} (${newEntry.account})`)
    }
    return
  }

  // 3. Partial fallback: only secret was encoded in QR / text, open drawer for user to fill issuer
  showAddModal.value = true
  formSecret.value = parsed.secret
  if (parsed.issuer) formIssuer.value = parsed.issuer
  if (parsed.account) formAccount.value = parsed.account
  formCategory.value = parsed.category
  store.showToast(`${sourceTag}已提取 Base32 密钥，请填写服务商名称后保存`, 'info')
}

/**
 * 1. Interactive Crosshair Screen Region QR Scan (Cmd+Shift+S or Scan Button)
 */
async function handleScreenQrScan() {
  if (isScanningScreen.value) return
  if (!isTauriEnv()) {
    store.showToast('当前处于浏览器预览环境，请使用「图片识别」或直接粘贴二维码图片', 'info')
    triggerImageFileSelect()
    return
  }

  try {
    isScanningScreen.value = true
    const dataUrl = await captureScreenRegionBase64()
    if (!dataUrl) {
      // User pressed Esc to cancel selection
      return
    }

    const qrText = await decodeQrFromImageSource(dataUrl)
    if (!qrText) {
      store.showToast('未在框选区域内识别到二维码，请确保完整框住二维码图案', 'warning')
      return
    }

    await processRecognizedPayload(qrText, '屏幕扫码')
  } catch (err) {
    store.showToast(`屏幕扫码失败：${(err as Error).message || '未知错误'}`, 'warning')
  } finally {
    isScanningScreen.value = false
  }
}

/**
 * 2. Decode QR code from an uploaded / dropped / pasted Image File or Blob
 */
async function handleImageBlobOrDataUrl(source: string | Blob, sourceLabel = '图片识别') {
  try {
    const qrText = await decodeQrFromImageSource(source)
    if (!qrText) {
      store.showToast(`未在${sourceLabel}中检测到清晰的二维码`, 'warning')
      return
    }
    await processRecognizedPayload(qrText, sourceLabel)
  } catch {
    store.showToast(`${sourceLabel}解码失败，请确认图片格式有效`, 'warning')
  }
}

function triggerImageFileSelect() {
  fileInputRef.value?.click()
}

async function handleFileInputChange(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  await handleImageBlobOrDataUrl(file, '本地图片')
  input.value = ''
}

/**
 * 3. Drag & Drop QR Code Image anywhere on the 2FA view
 */
function handleDragOver(e: DragEvent) {
  if (e.dataTransfer?.types.includes('Files')) {
    e.preventDefault()
    isDraggingImage.value = true
  }
}

function handleDragLeave(e: DragEvent) {
  if (e.currentTarget === e.target) {
    isDraggingImage.value = false
  }
}

async function handleDrop(e: DragEvent) {
  e.preventDefault()
  isDraggingImage.value = false
  const file = e.dataTransfer?.files?.[0]
  if (!file) return
  if (!file.type.startsWith('image/')) {
    store.showToast('请拖入包含 2FA 二维码的图片文件 (.png / .jpg / .webp)', 'warning')
    return
  }
  await handleImageBlobOrDataUrl(file, '拖拽图片')
}

/**
 * 4. Smart Clipboard Paste (Command+V button or global shortcut):
 * Supports pasting screenshot image from clipboard OR otpauth:// URI text!
 */
async function handleSmartClipboardImport() {
  // Step A: Try native macOS NSPasteboard image first (works with WeChat / Cmd+Ctrl+Shift+4 screenshots)
  const nativeImgDataUrl = await readClipboardImageBase64()
  if (nativeImgDataUrl) {
    const qrText = await decodeQrFromImageSource(nativeImgDataUrl)
    if (qrText) {
      await processRecognizedPayload(qrText, '剪贴板图片')
      return
    }
  }

  // Step B: Try Web Clipboard API text
  try {
    const clipText = (await navigator.clipboard.readText()).trim()
    if (clipText) {
      await processRecognizedPayload(clipText, '剪贴板粘贴')
      return
    }
  } catch {
    // ignore
  }

  if (nativeImgDataUrl) {
    store.showToast('剪贴板中的图片未识别出二维码', 'warning')
  } else {
    store.showToast('剪贴板中未检测到二维码截图或 otpauth:// 密钥链接', 'info')
  }
}

async function handleGlobalPasteEvent(e: ClipboardEvent) {
  const target = e.target as HTMLElement | null
  const isEditingInput =
    target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA')

  // 1. Check if clipboard event contains an image blob
  const items = e.clipboardData?.items
  if (items) {
    for (let i = 0; i < items.length; i++) {
      const item = items[i]!
      if (item.type.startsWith('image/')) {
        const blob = item.getAsFile()
        if (blob) {
          e.preventDefault()
          await handleImageBlobOrDataUrl(blob, '粘贴图片')
          return
        }
      }
    }
  }

  // 2. If user is typing inside an input box (except pasting full otpauth:// URI into otpauthUriInput), let normal input work
  const pastedText = e.clipboardData?.getData('text/plain')?.trim() || ''
  if (isEditingInput) {
    if (/^otpauth:\/\//i.test(pastedText)) {
      e.preventDefault()
      await processRecognizedPayload(pastedText, '粘贴链接')
    }
    return
  }

  // 3. When not focused in an input box, pressing Cmd+V anywhere in 2FA plugin triggers smart paste
  if (pastedText && (/^otpauth:\/\//i.test(pastedText) || pastedText.length >= 12)) {
    e.preventDefault()
    await processRecognizedPayload(pastedText, '粘贴导入')
    return
  }

  // 4. Fallback to native macOS NSPasteboard image (e.g. TIFF screenshot from system screenshot tool)
  const nativeImg = await readClipboardImageBase64()
  if (nativeImg) {
    e.preventDefault()
    await handleImageBlobOrDataUrl(nativeImg, '粘贴截图')
  }
}

function handlePluginKeydown(e: KeyboardEvent) {
  // Cmd+Shift+S / Ctrl+Shift+S triggers interactive screen QR code scan
  if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 's') {
    e.preventDefault()
    e.stopPropagation()
    handleScreenQrScan()
  }
}

function parseOtpauthUri() {
  const raw = otpauthUriInput.value.trim()
  if (!raw) {
    store.showToast('请先粘贴 otpauth:// 链接或点击右侧「屏幕扫码」', 'warning')
    return
  }
  processRecognizedPayload(raw, 'URI 解析')
}

async function handleAddEntry() {
  const cleanedSecret = sanitizeBase32Secret(formSecret.value)
  if (!formIssuer.value.trim() || !cleanedSecret) {
    store.showToast('请填写平台名称与合法的 Base32 密钥 (A-Z, 2-7)', 'warning')
    return
  }

  const existing = store.totpEntries.find(
    (item) => sanitizeBase32Secret(item.secret) === cleanedSecret
  )
  if (existing) {
    flashHighlightCard(existing.id)
    store.showToast(`该密钥已存在：${existing.issuer} (${existing.account})`, 'info')
    return
  }

  const issuerName = formIssuer.value.trim()
  const accountName = formAccount.value.trim() || 'default@account'
  const newEntry: TotpEntry = {
    id: `totp-${Date.now()}`,
    issuer: issuerName,
    account: accountName,
    secret: cleanedSecret,
    period: 30,
    digits: 6,
    category: formCategory.value || inferCategoryFromIssuer(issuerName, accountName)
  }

  await store.saveTotpEntries([newEntry, ...store.totpEntries])
  formIssuer.value = ''
  formAccount.value = ''
  formSecret.value = ''
  otpauthUriInput.value = ''
  showAddModal.value = false
  flashHighlightCard(newEntry.id)
  store.showToast(`已添加 ${newEntry.issuer} 2FA 令牌`)
}

async function removeEntry(id: string) {
  const next = store.totpEntries.filter((e) => e.id !== id)
  await store.saveTotpEntries(next)
  if (activeQrModalEntry.value?.id === id) {
    activeQrModalEntry.value = null
  }
  store.showToast('已移除该 2FA 令牌', 'info')
}

/**
 * 5. Open Reverse QR Code Modal for mobile Authenticator scanning
 */
async function openEntryQrModal(entry: TotpEntry) {
  try {
    showRawSecretInModal.value = false
    const { uri, qrDataUrl } = await buildOtpauthQrCode(entry)
    activeQrOtpUri.value = uri
    activeQrDataUrl.value = qrDataUrl
    activeQrModalEntry.value = entry
  } catch {
    store.showToast('生成二维码失败', 'warning')
  }
}

function downloadEntryQrImage() {
  if (!activeQrDataUrl.value || !activeQrModalEntry.value) return
  const a = document.createElement('a')
  a.href = activeQrDataUrl.value
  const safeIssuer = activeQrModalEntry.value.issuer.replace(/[^a-zA-Z0-9_-]/g, '_')
  a.download = `omnibox-2fa-${safeIssuer}.png`
  a.click()
  store.showToast(`已下载 ${activeQrModalEntry.value.issuer} 二维码图片`)
}

onMounted(() => {
  const winObj = window as unknown as {
    __omniboxPluginInterceptEscape?: () => boolean
  }
  winObj.__omniboxPluginInterceptEscape = () => {
    if (activeQrModalEntry.value) {
      activeQrModalEntry.value = null
      return true
    }
    if (showAddModal.value) {
      showAddModal.value = false
      return true
    }
    return false
  }
  window.addEventListener('paste', handleGlobalPasteEvent)
  window.addEventListener('keydown', handlePluginKeydown, true)
})

onUnmounted(() => {
  const winObj = window as unknown as {
    __omniboxPluginInterceptEscape?: () => boolean
  }
  if (winObj.__omniboxPluginInterceptEscape) {
    delete winObj.__omniboxPluginInterceptEscape
  }
  window.removeEventListener('paste', handleGlobalPasteEvent)
  window.removeEventListener('keydown', handlePluginKeydown, true)
})
</script>

<template>
  <div
    class="relative flex flex-col flex-1 h-full min-h-[500px] select-none overflow-hidden"
    :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
    @dragover="handleDragOver"
    @dragleave="handleDragLeave"
    @drop="handleDrop"
  >
    <!-- Hidden File Input for Local QR Code Image Upload -->
    <input
      ref="fileInputRef"
      type="file"
      accept="image/*"
      class="hidden"
      @change="handleFileInputChange"
    />

    <!-- Drag & Drop QR Image Overlay -->
    <div
      v-if="isDraggingImage"
      class="absolute inset-3 z-40 rounded-2xl border-2 border-dashed border-indigo-500 bg-indigo-500/10 backdrop-blur-sm flex flex-col items-center justify-center gap-2 pointer-events-none"
    >
      <div class="w-12 h-12 rounded-2xl bg-indigo-600 text-white flex items-center justify-center shadow-lg">
        <IconRenderer name="QrCode" :size="24" />
      </div>
      <p class="text-sm font-bold text-indigo-500">松开鼠标即可识别二维码图片并自动添加 2FA 令牌</p>
      <p class="text-xs text-slate-400">支持 PNG / JPG / WebP 截图或备份二维码文件</p>
    </div>

    <!-- Top Header Bar -->
    <div
      class="flex items-center justify-between px-4 py-2.5 border-b text-xs shrink-0"
      :class="
        store.isDark
          ? 'bg-obsidian-950/60 border-slate-800/80'
          : 'bg-slate-50/90 border-slate-200/80'
      "
    >
      <div class="flex items-center gap-2 text-slate-400 truncate pr-2">
        <IconRenderer name="ShieldCheck" :size="15" class-name="text-indigo-500 shrink-0" />
        <span class="truncate">
          RFC 6238 标准 TOTP • 支持屏幕框选扫码、⌘V 粘贴截图或拖拽二维码图片直接添加
        </span>
      </div>

      <div class="flex items-center gap-2 shrink-0">
        <!-- 1. Screen Region QR Code Scan Button (Primary One-Click Entry) -->
        <button
          type="button"
          title="框选屏幕上的二维码自动识别并添加 (⌘⇧S)"
          :disabled="isScanningScreen"
          class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold border cursor-pointer transition-all disabled:opacity-50"
          :class="
            store.isDark
              ? 'bg-indigo-500/15 hover:bg-indigo-500/25 border-indigo-500/40 text-indigo-300'
              : 'bg-indigo-50 hover:bg-indigo-100/80 border-indigo-200 text-indigo-600'
          "
          @click="handleScreenQrScan"
        >
          <IconRenderer name="ScanLine" :size="14" />
          <span>{{ isScanningScreen ? '正在框选...' : '扫码添加' }}</span>
        </button>

        <!-- 2. Paste Screenshot / URI from Clipboard -->
        <button
          type="button"
          title="识别剪贴板中的二维码截图或 otpauth:// 链接 (⌘V)"
          class="flex items-center gap-1 px-2.5 py-1.5 rounded-xl text-xs font-medium border cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-slate-800/80 hover:bg-slate-800 border-slate-700 text-slate-300'
              : 'bg-white hover:bg-slate-100 border-slate-200/90 text-slate-700'
          "
          @click="handleSmartClipboardImport"
        >
          <IconRenderer name="ClipboardPaste" :size="13" />
          <span>粘贴识别</span>
        </button>

        <!-- 3. Upload Local QR Image -->
        <button
          type="button"
          title="选择本地二维码图片识别 (.png / .jpg)"
          class="flex items-center gap-1 px-2.5 py-1.5 rounded-xl text-xs font-medium border cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-slate-800/80 hover:bg-slate-800 border-slate-700 text-slate-300'
              : 'bg-white hover:bg-slate-100 border-slate-200/90 text-slate-700'
          "
          @click="triggerImageFileSelect"
        >
          <IconRenderer name="ImageUp" :size="13" />
          <span>图片</span>
        </button>

        <!-- 4. Manual / URI Drawer Toggle -->
        <button
          type="button"
          class="btn-primary"
          @click="showAddModal = !showAddModal"
        >
          <IconRenderer :name="showAddModal ? 'X' : 'Plus'" :size="14" />
          <span>{{ showAddModal ? '取消添加' : '手动添加' }}</span>
        </button>
      </div>
    </div>

    <!-- Add New 2FA Drawer -->
    <div
      v-if="showAddModal"
      class="px-5 py-3.5 border-b space-y-2.5 shrink-0"
      :class="store.isDark ? 'bg-slate-900/95 border-slate-800' : 'bg-slate-100/80 border-slate-200'"
    >
      <div class="flex items-center gap-2">
        <input
          v-model="otpauthUriInput"
          type="text"
          placeholder="快捷导入：粘贴 otpauth://totp/GitHub:user?secret=JBSWY3DPEHPK3PXP&issuer=GitHub"
          aria-label="otpauth URI 快捷导入框"
          class="flex-1 px-3 py-1.5 rounded-xl text-xs font-mono border outline-none focus:ring-2 focus:ring-indigo-500/25 focus:border-indigo-500"
          :class="
            store.isDark
              ? 'bg-obsidian-950 border-slate-700 text-slate-200'
              : 'bg-white border-slate-300 text-slate-800'
          "
          @keydown.enter="parseOtpauthUri"
        />
        <button
          type="button"
          class="btn-secondary flex items-center gap-1"
          title="框选屏幕上的二维码自动识别"
          @click="handleScreenQrScan"
        >
          <IconRenderer name="ScanLine" :size="13" class-name="text-indigo-500" />
          <span>屏幕扫码</span>
        </button>
        <button
          type="button"
          class="btn-secondary"
          @click="parseOtpauthUri"
        >
          解析 URI
        </button>
      </div>

      <div class="grid grid-cols-4 gap-2.5 text-xs">
        <input
          v-model="formIssuer"
          type="text"
          placeholder="服务商 (如 GitHub / AWS)"
          aria-label="服务商名称"
          class="px-3 py-1.5 rounded-xl border outline-none focus:ring-2 focus:ring-indigo-500/25"
          :class="
            store.isDark
              ? 'bg-obsidian-950 border-slate-700 text-slate-200'
              : 'bg-white border-slate-300 text-slate-800'
          "
        />
        <input
          v-model="formAccount"
          type="text"
          placeholder="账号 (如 dev@domain.com)"
          aria-label="账户名称"
          class="px-3 py-1.5 rounded-xl border outline-none focus:ring-2 focus:ring-indigo-500/25"
          :class="
            store.isDark
              ? 'bg-obsidian-950 border-slate-700 text-slate-200'
              : 'bg-white border-slate-300 text-slate-800'
          "
        />
        <input
          v-model="formSecret"
          type="text"
          placeholder="Base32 密钥 (如 JBSWY3DP...)"
          aria-label="Base32 密钥"
          class="px-3 py-1.5 rounded-xl font-mono border outline-none focus:ring-2 focus:ring-indigo-500/25"
          :class="
            store.isDark
              ? 'bg-obsidian-950 border-slate-700 text-slate-200'
              : 'bg-white border-slate-300 text-slate-800'
          "
          @keydown.enter="handleAddEntry"
        />
        <button
          type="button"
          class="btn-primary"
          @click="handleAddEntry"
        >
          确认保存令牌
        </button>
      </div>
    </div>

    <!-- 2FA Cards Grid -->
    <div
      class="flex-1 overflow-y-auto p-4 grid grid-cols-2 gap-3 content-start"
      :class="store.isDark ? 'bg-transparent' : 'bg-[#F8FAFC]'"
    >
      <div
        v-for="item in filteredEntries"
        :key="item.id"
        class="group relative flex flex-col justify-between p-4 rounded-2xl border cursor-pointer transition-all duration-200"
        :class="[
          highlightedEntryId === item.id
            ? 'ring-2 ring-indigo-500 border-indigo-500 shadow-lg shadow-indigo-500/10'
            : '',
          store.isDark
            ? 'bg-obsidian-800/70 hover:bg-obsidian-800 border-slate-800 hover:border-indigo-500/40'
            : 'bg-white hover:border-indigo-300 border-slate-200/80 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
        ]"
        @click="copyCode(item)"
      >
        <!-- Top Row: Issuer + Category + Action Icons (QR Code Sync + Delete) -->
        <div class="flex items-start justify-between">
          <div class="min-w-0 pr-2">
            <div class="flex items-center gap-2">
              <span
                class="text-sm font-bold tracking-tight truncate"
                :class="store.isDark ? 'text-white' : 'text-slate-900'"
              >
                {{ item.issuer }}
              </span>
              <span
                class="px-1.5 py-0.5 text-[10px] rounded-md font-medium border shrink-0"
                :class="
                  store.isDark
                    ? 'bg-slate-800 border-slate-700 text-slate-400'
                    : 'bg-slate-100 border-slate-200/80 text-slate-600'
                "
              >
                {{ item.category }}
              </span>
            </div>
            <p class="text-xs text-slate-400 font-mono mt-0.5 truncate">{{ item.account }}</p>
          </div>

          <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
            <!-- Show QR Code for Mobile Sync -->
            <button
              type="button"
              title="展示二维码（供手机扫码同步或导出 otpauth 链接）"
              aria-label="展示同步二维码"
              class="p-1.5 rounded-lg text-slate-400 hover:text-indigo-500 hover:bg-indigo-500/10 transition-colors cursor-pointer"
              @click.stop="openEntryQrModal(item)"
            >
              <IconRenderer name="QrCode" :size="14" />
            </button>

            <!-- Delete Token -->
            <button
              type="button"
              title="删除此令牌"
              aria-label="删除令牌"
              class="p-1.5 rounded-lg text-slate-400 hover:text-rose-500 hover:bg-rose-500/10 transition-colors cursor-pointer"
              @click.stop="removeEntry(item.id)"
            >
              <IconRenderer name="Trash2" :size="14" />
            </button>
          </div>
        </div>

        <!-- Bottom Row: 6-digit Monospace Code + 30s Circular Countdown -->
        <div
          class="flex items-end justify-between mt-4 pt-3 border-t"
          :class="store.isDark ? 'border-slate-800/80' : 'border-slate-100'"
        >
          <div class="flex items-center gap-2">
            <span
              class="font-mono text-2xl font-bold tracking-widest"
              :class="
                (store.totpLiveMap[item.id]?.remaining || 30) <= 5
                  ? 'text-rose-500'
                  : store.isDark
                    ? 'text-indigo-300'
                    : 'text-indigo-600'
              "
            >
              {{ formatCode(store.totpLiveMap[item.id]?.code) }}
            </span>
            <span
              class="text-[11px] opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-1 text-slate-400"
            >
              <IconRenderer name="Copy" :size="12" />
              点击复制
            </span>
          </div>

          <!-- Circular SVG Countdown Indicator -->
          <div class="flex items-center gap-1.5 font-mono text-xs">
            <svg class="w-6 h-6 -rotate-90" viewBox="0 0 24 24">
              <circle
                cx="12"
                cy="12"
                r="9"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                :class="store.isDark ? 'text-slate-700/40' : 'text-slate-200'"
              />
              <circle
                cx="12"
                cy="12"
                r="9"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                :stroke-dasharray="56.5"
                :stroke-dashoffset="
                  56.5 - (56.5 * (store.totpLiveMap[item.id]?.remaining || 30)) / 30
                "
                class="transition-all duration-500"
                :class="
                  (store.totpLiveMap[item.id]?.remaining || 30) <= 5
                    ? 'text-rose-500'
                    : 'text-indigo-500'
                "
              />
            </svg>
            <span
              class="w-6 text-right font-semibold"
              :class="
                (store.totpLiveMap[item.id]?.remaining || 30) <= 5
                  ? 'text-rose-500'
                  : 'text-slate-400'
              "
            >
              {{ store.totpLiveMap[item.id]?.remaining || 30 }}s
            </span>
          </div>
        </div>
      </div>
    </div>

    <!-- ===================================================================== -->
    <!-- Reverse QR Code Sync & Export Modal (Mobile Authenticator Sync)       -->
    <!-- ===================================================================== -->
    <div
      v-if="activeQrModalEntry"
      class="absolute inset-0 z-50 flex items-center justify-center bg-slate-950/60 backdrop-blur-xs p-4"
      @click.self="activeQrModalEntry = null"
    >
      <div
        class="w-full max-w-[360px] rounded-2xl border p-5 shadow-2xl space-y-4"
        :class="
          store.isDark
            ? 'bg-[#111827] border-slate-800 text-slate-100'
            : 'bg-white border-slate-200 text-slate-800'
        "
      >
        <div class="flex items-start justify-between">
          <div>
            <div class="flex items-center gap-2">
              <h3 class="text-sm font-bold">{{ activeQrModalEntry.issuer }}</h3>
              <span
                class="px-1.5 py-0.5 text-[10px] rounded-md font-medium border"
                :class="
                  store.isDark
                    ? 'bg-slate-800 border-slate-700 text-slate-300'
                    : 'bg-slate-100 border-slate-200 text-slate-600'
                "
              >
                {{ activeQrModalEntry.category }}
              </span>
            </div>
            <p class="text-xs text-slate-400 font-mono mt-0.5">{{ activeQrModalEntry.account }}</p>
          </div>
          <button
            type="button"
            class="p-1 rounded-lg text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
            @click="activeQrModalEntry = null"
          >
            <IconRenderer name="X" :size="15" />
          </button>
        </div>

        <!-- High-Contrast White QR Canvas Card -->
        <div class="flex flex-col items-center justify-center p-3.5 rounded-2xl bg-white border border-slate-200/80 shadow-inner">
          <img
            v-if="activeQrDataUrl"
            :src="activeQrDataUrl"
            :alt="`${activeQrModalEntry.issuer} 2FA QR Code`"
            class="w-48 h-48 object-contain select-none"
          />
          <p class="text-[11px] text-slate-500 mt-1.5">
            使用手机 Authenticator 扫描上方二维码即可同步此令牌
          </p>
        </div>

        <!-- Secret Key Preview / Toggle -->
        <div
          class="flex items-center justify-between px-3 py-2 rounded-xl border text-xs font-mono"
          :class="
            store.isDark
              ? 'bg-slate-900/90 border-slate-800 text-slate-300'
              : 'bg-slate-50 border-slate-200/80 text-slate-700'
          "
        >
          <span class="truncate pr-2">
            {{
              showRawSecretInModal
                ? activeQrModalEntry.secret
                : activeQrModalEntry.secret.slice(0, 4) + ' •••• •••• ' + activeQrModalEntry.secret.slice(-4)
            }}
          </span>
          <div class="flex items-center gap-1.5 shrink-0">
            <button
              type="button"
              class="text-slate-400 hover:text-indigo-500 cursor-pointer"
              :title="showRawSecretInModal ? '隐藏密钥' : '显示明文密钥'"
              @click="showRawSecretInModal = !showRawSecretInModal"
            >
              <IconRenderer :name="showRawSecretInModal ? 'EyeOff' : 'Eye'" :size="13" />
            </button>
            <button
              type="button"
              class="text-indigo-500 hover:underline cursor-pointer text-[11px] font-sans"
              @click="copyPlainText(activeQrModalEntry.secret, '已复制 Base32 密钥')"
            >
              复制密钥
            </button>
          </div>
        </div>

        <!-- Bottom Action Buttons -->
        <div class="grid grid-cols-2 gap-2 text-xs">
          <button
            type="button"
            class="btn-secondary flex items-center justify-center gap-1.5 py-2"
            @click="copyPlainText(activeQrOtpUri, '已复制标准 otpauth:// 链接')"
          >
            <IconRenderer name="Copy" :size="13" />
            <span>复制 otpauth 链接</span>
          </button>
          <button
            type="button"
            class="btn-primary flex items-center justify-center gap-1.5 py-2"
            @click="downloadEntryQrImage"
          >
            <IconRenderer name="Download" :size="13" />
            <span>下载二维码图片</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
