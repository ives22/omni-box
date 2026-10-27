import { invoke } from '@tauri-apps/api/core'
import { pinyin } from 'pinyin-pro'

export interface SearchResultItem {
  id: string
  title: string
  subtitle: string
  category: 'plugin' | 'app' | 'command' | 'file' | 'totp' | 'vault' | 'calc' | 'clipboard'
  icon: string
  icon_data_url?: string | null
  keywords: string[]
  action_payload: string
  score: number
  match_type: string
  badge?: string
  totpCode?: string
  totpRemaining?: number
}

export interface SystemAppItem {
  id: string
  title: string
  subtitle: string
  category: 'app'
  icon: string
  icon_data_url?: string | null
  keywords: string[]
  action_payload: string
}

export interface TotpEntry {
  id: string
  issuer: string
  account: string
  secret: string
  period: number
  digits: number
  category: string
}

export type VaultTemplateType = 'general' | 'cloud' | 'database'
export type VaultEnvType = 'PROD' | 'PREV' | 'TEST' | 'DEV' | 'COMMON'

export interface VaultSubAccount {
  id: string
  role: string
  username: string
  password: string
  accessKeyId?: string
  accessKeySecret?: string
  extraSecret?: string
  notes?: string
}

export interface VaultEntry {
  id: string
  title: string
  username: string
  password: string
  url: string
  category: string
  notes: string
  updatedAt: number
  // Extended multi-dimensional fields for DevOps / Cloud / DB management
  templateType?: VaultTemplateType
  env?: VaultEnvType
  componentType?: string
  moduleName?: string
  publicAddress?: string
  privateAddress?: string
  accessKeyId?: string
  accessKeySecret?: string
  extraSecret?: string
  subAccounts?: VaultSubAccount[]
}

export interface ExternalUtoolsPlugin {
  id: string
  pluginName: string
  description: string
  version: string
  author: string
  main: string
  cmds: string[]
  installedAt: number
  customHtml?: string
}

export interface UserSettings {
  nickname: string
  roleTitle: string
  globalHotkey: string
  hideOnBlur: boolean
  smartClipboard: boolean
  directTotpInSearch: boolean
  vaultAutoLockMinutes: number
  accentColor: 'emerald' | 'sky' | 'violet' | 'amber'
  maxResultsCount: number
  aliases: Record<string, string> // alias -> target item.id
  disabledAppIds: string[]
  webdavUrl: string
  webdavUsername: string
  webdavPassword: string
  lastSyncAt: number | null
  // Extended Preference Settings (uTools style)
  searchPlaceholder?: string
  enableSuperPanel?: boolean
  superPanelMouseTrigger?: string
  superPanelHoldMs?: number
  quickTranslate?: boolean
  voiceInteraction?: boolean
  autoPinFeature?: boolean
  blockedSuperPanelApps?: string
  themeMode?: 'system' | 'dark' | 'light'
  windowOpacity?: number
  launchAtLogin?: boolean
  showRecentInSearch?: boolean
  showMarketFeatured?: boolean
  showAppRecommendation?: boolean
  detachWindowHotkey?: string
  searchBarMode?: '聚合模式' | '列表模式'
  autoPasteWindow?: string
  autoClearWindow?: string
  spaceToOpen?: boolean
  proxyEnabled?: boolean
  proxyHost?: string
  aiProvider?: string
  aiBaseUrl?: string
  aiApiKey?: string
  aiModelName?: string
  aiQuickAskEnabled?: boolean
}

export const DEFAULT_USER_SETTINGS: UserSettings = {
  nickname: 'OmniBox Pro User',
  roleTitle: 'Local-First 极客工作台 • 无插件数量限制',
  globalHotkey: 'Option + Space',
  hideOnBlur: true,
  smartClipboard: true,
  directTotpInSearch: true,
  vaultAutoLockMinutes: 15,
  accentColor: 'emerald',
  maxResultsCount: 14,
  aliases: {
    cc: 'plugin:calc-pad',
    jj: 'plugin:json-studio',
    dd: 'plugin:diff-viewer',
    vv: 'plugin:password-vault',
    ss: 'plugin:user-center'
  },
  disabledAppIds: [],
  webdavUrl: 'https://dav.jianguoyun.com/dav/OmniBox/',
  webdavUsername: '',
  webdavPassword: '',
  lastSyncAt: null,
  searchPlaceholder: '搜索功能 / 本机应用 / 粘贴文本或输入算式',
  enableSuperPanel: true,
  superPanelMouseTrigger: '长按鼠标右键',
  superPanelHoldMs: 250,
  quickTranslate: true,
  voiceInteraction: false,
  autoPinFeature: true,
  blockedSuperPanelApps: '',
  themeMode: 'light',
  windowOpacity: 98,
  launchAtLogin: true,
  showRecentInSearch: true,
  showMarketFeatured: true,
  showAppRecommendation: true,
  detachWindowHotkey: 'Command+D',
  searchBarMode: '聚合模式',
  autoPasteWindow: '5 秒内',
  autoClearWindow: '3 分钟',
  spaceToOpen: false,
  proxyEnabled: false,
  proxyHost: 'http://127.0.0.1:7890',
  aiProvider: 'Google Gemini',
  aiBaseUrl: 'https://generativelanguage.googleapis.com',
  aiApiKey: '',
  aiModelName: 'gemini-2.5-pro',
  aiQuickAskEnabled: true
}

export const isTauriEnv = (): boolean => {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

export const CORE_PLUGIN_ITEMS: Omit<SearchResultItem, 'score' | 'match_type'>[] = [
  {
    id: 'plugin:calc-pad',
    title: '计算稿纸 (Calc Pad)',
    subtitle: '支持多行变量赋值、单位换算、进制转换与实时聚合求和的智能计算笔记',
    category: 'plugin',
    icon: 'Calculator',
    keywords: ['calc', 'math', 'numi', '计算稿纸', '计算器', '单位换算', '稿纸', 'jsgz'],
    action_payload: 'calc-pad',
    badge: '内置极速'
  },
  {
    id: 'plugin:json-studio',
    title: 'JSON 助手 (JSON Studio)',
    subtitle: '极速 JSON 格式化、压缩、转义、JSONPath 实时过滤与 TS/Rust/Go 结构体生成',
    category: 'plugin',
    icon: 'Braces',
    keywords: ['json', 'format', 'jsonpath', 'struct', '格式化', '转义', 'jsonzs'],
    action_payload: 'json-studio',
    badge: '内置极速'
  },
  {
    id: 'plugin:diff-viewer',
    title: '文本对比 (Diff Viewer)',
    subtitle: '左右双栏与行内字符级代码/文本差异对比工具，支持忽略空白与差异块定位',
    category: 'plugin',
    icon: 'GitCompare',
    keywords: ['diff', 'compare', '文本对比', '代码对比', '差异', 'wbdb'],
    action_payload: 'diff-viewer',
    badge: '内置极速'
  },
  {
    id: 'plugin:authenticator',
    title: '2FA 验证码管理 (Authenticator)',
    subtitle: 'RFC 6238 TOTP 双重认证动态码管理器，支持 otpauth 导入与主搜索框直达复制',
    category: 'plugin',
    icon: 'ShieldCheck',
    keywords: ['2fa', 'totp', 'otp', 'auth', '验证码', '双重认证', '令牌', 'yzm'],
    action_payload: 'authenticator',
    badge: '内置极速'
  },
  {
    id: 'plugin:password-vault',
    title: '密码本 (Password Vault)',
    subtitle: '基于 Argon2id + AES-256-GCM 硬件级加密的本地密码保险箱与强密码生成器',
    category: 'plugin',
    icon: 'KeyRound',
    keywords: ['password', 'vault', 'key', '密码本', '保险箱', '账号', 'mmb'],
    action_payload: 'password-vault',
    badge: '安全加密'
  },
  {
    id: 'plugin:utools-hub',
    title: 'uTools 插件中心与沙箱挂载',
    subtitle: '突破 10 个应用限制，无限挂载本地与开源 uTools 生态插件 (plugin.json)',
    category: 'plugin',
    icon: 'Blocks',
    keywords: ['utools', 'rubick', 'plugin', '插件中心', '扩展', 'cjzx'],
    action_payload: 'utools-hub',
    badge: '生态兼容'
  },
  {
    id: 'plugin:user-center',
    title: '个人中心与偏好设置 (User Center)',
    subtitle: '管理本机聚焦应用索引、全局快捷键、自定义别名、WebDAV 同步、安全策略与主题外观',
    category: 'plugin',
    icon: 'SlidersHorizontal',
    keywords: ['settings', 'profile', 'preferences', '个人中心', '设置', '同步', '快捷键', 'grzx', 'sz'],
    action_payload: 'user-center',
    badge: '控制中枢'
  }
]

export const SYSTEM_COMMAND_ITEMS: Omit<SearchResultItem, 'score' | 'match_type'>[] = [
  {
    id: 'cmd:copy-local-ip',
    title: '查询并复制本机局域网 IP (Local IP)',
    subtitle: '读取 macOS en0 网卡 IPv4 地址并自动复制到剪贴板',
    category: 'command',
    icon: 'Globe',
    keywords: ['ip', 'network', '局域网', '内网ip', 'benjiip'],
    action_payload: 'sys:copy-local-ip',
    badge: '系统指令'
  },
  {
    id: 'cmd:toggle-dark-mode',
    title: '切换 macOS 系统深色/浅色外观',
    subtitle: '调用 AppleScript 立即切换 macOS 全局 Dark / Light 模式',
    category: 'command',
    icon: 'Moon',
    keywords: ['dark', 'light', 'theme', '深色模式', '暗黑模式', '外观'],
    action_payload: 'sys:toggle-dark-mode',
    badge: '系统指令'
  },
  {
    id: 'cmd:empty-trash',
    title: '清空 macOS 废纸篓 (Empty Trash)',
    subtitle: '调用 Finder 清空废纸篓释放磁盘空间',
    category: 'command',
    icon: 'Trash2',
    keywords: ['trash', 'clean', '清空废纸篓', '垃圾桶', 'qkfzl'],
    action_payload: 'sys:empty-trash',
    badge: '系统指令'
  },
  {
    id: 'cmd:lock-screen',
    title: '立即锁定屏幕 (Lock Screen)',
    subtitle: '立即休眠显示器并进入系统锁屏状态',
    category: 'command',
    icon: 'Lock',
    keywords: ['lock', 'sleep', '锁屏', '休眠', 'sp'],
    action_payload: 'sys:lock-screen',
    badge: '系统指令'
  }
]

const FALLBACK_MAC_APPS: SystemAppItem[] = [
  {
    id: 'app:wechat',
    title: '微信 (WeChat)',
    subtitle: '/Applications/WeChat.app',
    category: 'app',
    icon: 'AppWindow',
    keywords: ['WeChat', '微信', 'wx', 'weixin'],
    action_payload: '/Applications/WeChat.app'
  },
  {
    id: 'app:vscode',
    title: 'VS Code 编辑器 (Visual Studio Code)',
    subtitle: '/Applications/Visual Studio Code.app',
    category: 'app',
    icon: 'TerminalSquare',
    keywords: ['Visual Studio Code', 'vscode', 'code', '编辑器'],
    action_payload: '/Applications/Visual Studio Code.app'
  },
  {
    id: 'app:system-settings',
    title: '系统设置 (System Settings)',
    subtitle: '/System/Applications/System Settings.app',
    category: 'app',
    icon: 'SlidersHorizontal',
    keywords: ['System Settings', '系统设置', '偏好设置', 'xtsz'],
    action_payload: '/System/Applications/System Settings.app'
  },
  {
    id: 'app:activity-monitor',
    title: '活动监视器 (Activity Monitor)',
    subtitle: '/System/Applications/Utilities/Activity Monitor.app',
    category: 'app',
    icon: 'AppWindow',
    keywords: ['Activity Monitor', '活动监视器', '任务管理器', 'hdjsq'],
    action_payload: '/System/Applications/Utilities/Activity Monitor.app'
  },
  {
    id: 'app:terminal',
    title: '终端 (Terminal)',
    subtitle: '/System/Applications/Utilities/Terminal.app',
    category: 'app',
    icon: 'Terminal',
    keywords: ['Terminal', '终端', '命令行', 'zd'],
    action_payload: '/System/Applications/Utilities/Terminal.app'
  },
  {
    id: 'app:chrome',
    title: 'Google Chrome 浏览器 (Google Chrome)',
    subtitle: '/Applications/Google Chrome.app',
    category: 'app',
    icon: 'Globe',
    keywords: ['Google Chrome', 'chrome', '浏览器', '谷歌', 'llq'],
    action_payload: '/Applications/Google Chrome.app'
  }
]

export async function fetchSystemApps(): Promise<SystemAppItem[]> {
  if (isTauriEnv()) {
    try {
      const list = await invoke<SystemAppItem[]>('get_system_apps')
      if (Array.isArray(list) && list.length > 0) {
        return list
      }
    } catch {
      // fallback
    }
  }
  return FALLBACK_MAC_APPS
}

export async function triggerRescanSystemApps(clearIconCache = false): Promise<SystemAppItem[]> {
  if (isTauriEnv()) {
    try {
      return await invoke<SystemAppItem[]>('rescan_system_apps', { clearIconCache })
    } catch {
      // fallback
    }
  }
  return FALLBACK_MAC_APPS
}

export async function searchSpotlightLocalFiles(query: string): Promise<SearchResultItem[]> {
  if (isTauriEnv()) {
    try {
      return await invoke<SearchResultItem[]>('search_local_files', { query })
    } catch {
      return []
    }
  }
  // Simulated Spotlight file search results in browser preview mode
  const q = query.trim().toLowerCase()
  const mockFiles = [
    { name: 'OmniBox-Architecture-v2.md', path: '~/Documents/OmniBox-Architecture-v2.md', isDir: false },
    { name: 'Cargo.toml', path: '~/Projects/omni-box/src-tauri/Cargo.toml', isDir: false },
    { name: 'package.json', path: '~/Projects/omni-box/package.json', isDir: false },
    { name: 'Downloads', path: '~/Downloads', isDir: true }
  ]
  return mockFiles
    .filter((f) => f.name.toLowerCase().includes(q) || f.path.toLowerCase().includes(q))
    .map((f, idx) => ({
      id: `file:${idx}:${f.name}`,
      title: f.name,
      subtitle: f.path,
      category: 'file',
      icon: f.isDir ? 'FolderGit2' : 'FileCode2',
      keywords: [f.name],
      action_payload: f.path,
      score: 900 - idx * 10,
      match_type: 'Spotlight 文件',
      badge: f.isDir ? '文件夹' : '本地文件'
    }))
}

export async function revealPathInFinder(targetPath: string): Promise<void> {
  if (isTauriEnv()) {
    await invoke('reveal_in_finder', { targetPath })
  }
}

export async function runSystemCommand(cmdId: string): Promise<string> {
  if (isTauriEnv()) {
    return await invoke<string>('execute_system_command', { cmdId })
  }
  if (cmdId === 'sys:copy-local-ip') {
    return '192.168.1.108'
  }
  return `已执行系统指令 (${cmdId})`
}

// Pre-compute Pinyin full & initials for sub-millisecond matching
const pinyinCache = new Map<string, { full: string; initials: string }>()
function getPinyinIndex(text: string): { full: string; initials: string } {
  const cached = pinyinCache.get(text)
  if (cached) return cached
  const full = pinyin(text, { toneType: 'none', type: 'array' }).join('').toLowerCase()
  const initials = pinyin(text, { pattern: 'first', toneType: 'none', type: 'array' }).join('').toLowerCase()
  const entry = { full, initials }
  pinyinCache.set(text, entry)
  return entry
}

function subsequenceScore(haystack: string, needle: string): number | null {
  if (!needle) return 10
  if (needle.length > haystack.length) return null
  let score = 0
  let hIdx = 0
  let consecutive = 0

  for (let i = 0; i < needle.length; i++) {
    const nc = needle[i]
    let found = false
    while (hIdx < haystack.length) {
      if (haystack[hIdx] === nc) {
        found = true
        if (i === 0 && hIdx === 0) score += 80
        score += consecutive > 0 ? 35 * consecutive : 15
        consecutive++
        hIdx++
        break
      } else {
        consecutive = 0
        hIdx++
      }
    }
    if (!found) return null
  }
  return score + Math.max(0, 100 - haystack.length)
}

function decodeBase32(input: string): Uint8Array {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
  const cleaned = input.toUpperCase().replace(/[\s=-]/g, '')
  let bits = 0
  let value = 0
  const out: number[] = []

  for (let i = 0; i < cleaned.length; i++) {
    const idx = alphabet.indexOf(cleaned[i])
    if (idx === -1) continue
    value = (value << 5) | idx
    bits += 5
    if (bits >= 8) {
      bits -= 8
      out.push((value >> bits) & 0xff)
    }
  }
  return new Uint8Array(out)
}

export async function generateTotpCode(
  secretBase32: string,
  period = 30,
  digits = 6
): Promise<{ code: string; remaining: number }> {
  if (isTauriEnv()) {
    try {
      return await invoke<{ code: string; remaining: number }>('compute_totp', {
        secret: secretBase32,
        period,
        digits
      })
    } catch {
      // Fallback to WebCrypto
    }
  }

  const nowSec = Math.floor(Date.now() / 1000)
  const remaining = period - (nowSec % period)
  const counter = Math.floor(nowSec / period)

  const keyBytes = decodeBase32(secretBase32)
  if (keyBytes.length === 0) {
    return { code: '000000', remaining }
  }

  const counterBuf = new ArrayBuffer(8)
  const view = new DataView(counterBuf)
  view.setUint32(0, Math.floor(counter / 0x100000000), false)
  view.setUint32(4, counter & 0xffffffff, false)

  const cryptoKey = await crypto.subtle.importKey(
    'raw',
    keyBytes as unknown as BufferSource,
    { name: 'HMAC', hash: 'SHA-1' },
    false,
    ['sign']
  )
  const sig = new Uint8Array(await crypto.subtle.sign('HMAC', cryptoKey, counterBuf))
  const offset = sig[sig.length - 1] & 0x0f
  const binary =
    ((sig[offset] & 0x7f) << 24) |
    ((sig[offset + 1] & 0xff) << 16) |
    ((sig[offset + 2] & 0xff) << 8) |
    (sig[offset + 3] & 0xff)

  const otp = binary % Math.pow(10, digits)
  return {
    code: otp.toString().padStart(digits, '0'),
    remaining
  }
}

export async function encryptVaultData(masterPassword: string, plaintext: string): Promise<string> {
  if (isTauriEnv()) {
    return await invoke<string>('vault_encrypt_payload', { masterPassword, plaintext })
  }

  const enc = new TextEncoder()
  const salt = crypto.getRandomValues(new Uint8Array(16))
  const nonce = crypto.getRandomValues(new Uint8Array(12))
  const baseKey = await crypto.subtle.importKey('raw', enc.encode(masterPassword), 'PBKDF2', false, ['deriveKey'])
  const aesKey = await crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt, iterations: 100000, hash: 'SHA-256' },
    baseKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt']
  )
  const cipherBuf = await crypto.subtle.encrypt({ name: 'AES-GCM', iv: nonce }, aesKey, enc.encode(plaintext))
  const toB64 = (u8: Uint8Array) => btoa(String.fromCharCode(...u8))
  return JSON.stringify({
    salt: toB64(salt),
    nonce: toB64(nonce),
    ciphertext: toB64(new Uint8Array(cipherBuf))
  })
}

export async function decryptVaultData(masterPassword: string, packetJson: string): Promise<string> {
  if (isTauriEnv()) {
    return await invoke<string>('vault_decrypt_payload', { masterPassword, packetJson })
  }

  const parsed = JSON.parse(packetJson)
  const fromB64 = (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0))
  const salt = fromB64(parsed.salt)
  const nonce = fromB64(parsed.nonce)
  const ciphertext = fromB64(parsed.ciphertext)

  const enc = new TextEncoder()
  const baseKey = await crypto.subtle.importKey('raw', enc.encode(masterPassword), 'PBKDF2', false, ['deriveKey'])
  const aesKey = await crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt, iterations: 100000, hash: 'SHA-256' },
    baseKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['decrypt']
  )
  const plainBuf = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: nonce }, aesKey, ciphertext)
  return new TextDecoder().decode(plainBuf)
}

export async function kvGet<T>(pluginId: string, docId: string, fallback: T): Promise<T> {
  const key = `omnibox:kv:${pluginId}:${docId}`
  if (isTauriEnv()) {
    try {
      const raw = await invoke<string | null>('kv_storage_get', { pluginId, docId })
      if (raw) return JSON.parse(raw) as T
    } catch {
      // Fallback to localStorage
    }
  }
  const localRaw = localStorage.getItem(key)
  if (!localRaw) return fallback
  try {
    return JSON.parse(localRaw) as T
  } catch {
    return fallback
  }
}

export async function kvPut<T>(pluginId: string, docId: string, value: T): Promise<void> {
  const key = `omnibox:kv:${pluginId}:${docId}`
  const json = JSON.stringify(value)
  localStorage.setItem(key, json)
  if (isTauriEnv()) {
    try {
      await invoke('kv_storage_put', { pluginId, docId, dataJson: json })
    } catch {
      // already persisted to localStorage
    }
  }
}

export function getFrecencyMap(): Record<string, number> {
  try {
    return JSON.parse(localStorage.getItem('omnibox:frecency') || '{}')
  } catch {
    return {}
  }
}

export async function recordUsage(itemId: string): Promise<void> {
  const map = getFrecencyMap()
  map[itemId] = (map[itemId] || 0) + 1
  localStorage.setItem('omnibox:frecency', JSON.stringify(map))
  if (isTauriEnv()) {
    try {
      await invoke('record_item_usage', { itemId })
    } catch {
      // ignore
    }
  }
}

const QUICK_EN_ZH_DICT: Record<string, string> = {
  hello: '你好；哈喽',
  world: '世界；领域',
  preferences: '偏好设置；首选项',
  settings: '设置；配置项',
  architecture: '架构；体系结构',
  concurrency: '并发；并发性',
  asynchronous: '异步的 (async)',
  synchronous: '同步的 (sync)',
  deprecated: '已弃用的；不推荐使用的',
  repository: '代码仓库；存储库',
  authentication: '身份验证；认证 (AuthN)',
  authorization: '授权；权限控制 (AuthZ)',
  middleware: '中间件',
  payload: '有效载荷；请求数据体',
  latency: '延迟；响应耗时',
  throughput: '吞吐量',
  idempotent: '幂等的（多次执行结果一致）',
  serialization: '序列化',
  deserialization: '反序列化',
  immutable: '不可变的',
  mutable: '可变的',
  refactor: '重构代码',
  deployment: '部署；发布',
  environment: '环境；环境变量',
  configuration: '配置；设置',
  dependency: '依赖项',
  vulnerability: '安全漏洞；脆弱性',
  encryption: '加密',
  decryption: '解密'
}

export async function quickTranslateEnglish(input: string): Promise<string | null> {
  const cleaned = input.trim()
  if (!cleaned || cleaned.length < 2 || cleaned.length > 160) return null
  // Only trigger for pure English words/sentences (letters, spaces, basic punctuation)
  if (!/^[a-zA-Z][a-zA-Z\s\-_',.?!]+$/.test(cleaned)) return null

  const lower = cleaned.toLowerCase()
  if (QUICK_EN_ZH_DICT[lower]) {
    return QUICK_EN_ZH_DICT[lower]
  }

  try {
    const controller = new AbortController()
    const timer = setTimeout(() => controller.abort(), 1500)
    const url = `https://translate.googleapis.com/translate_a/single?client=gtx&sl=en&tl=zh-CN&dt=t&q=${encodeURIComponent(
      cleaned
    )}`
    const res = await fetch(url, { signal: controller.signal })
    clearTimeout(timer)
    if (res.ok) {
      const data = await res.json()
      const translated = data?.[0]?.map((seg: unknown[]) => seg?.[0]).join('')
      if (translated && translated.toLowerCase() !== lower) {
        return translated
      }
    }
  } catch {
    // offline or timeout
  }
  return null
}

export const MARKET_RECOMMENDATION_ITEMS: Omit<SearchResultItem, 'score' | 'match_type'>[] = [
  {
    id: 'market-rec:timestamp',
    title: '时间戳转换器 (Timestamp Master)',
    subtitle: '应用推荐 · 秒级/毫秒级 Unix 时间戳与 ISO 日期互转插件（点击前往插件市场安装）',
    category: 'plugin',
    icon: 'Clock',
    keywords: ['timestamp', 'time', 'date', '时间戳', '日期转换', 'sjc'],
    action_payload: 'utools-hub',
    badge: '应用推荐'
  },
  {
    id: 'market-rec:codec',
    title: '编码解码瑞士军刀 (Base64 / URL / JWT)',
    subtitle: '应用推荐 · Base64、URL、JWT Token、Unicode、Hex 一键编解码工具',
    category: 'plugin',
    icon: 'Code2',
    keywords: ['base64', 'url', 'jwt', 'encode', 'decode', '编码', '解码', '加密'],
    action_payload: 'utools-hub',
    badge: '应用推荐'
  },
  {
    id: 'market-rec:regex',
    title: '正则表达式可视化测试 (Regex Lab)',
    subtitle: '应用推荐 · 实时高亮匹配分组与常用正则模板库（邮箱/手机号/IP/URL）',
    category: 'plugin',
    icon: 'Sparkles',
    keywords: ['regex', 'regexp', '正则表达式', '匹配', 'zzbds'],
    action_payload: 'utools-hub',
    badge: '应用推荐'
  },
  {
    id: 'market-rec:color',
    title: '屏幕取色与色彩空间转换 (HEX / RGB / OKLCH)',
    subtitle: '应用推荐 · 设计师与前端必备色彩换算与 Tailwind 色板速查工具',
    category: 'plugin',
    icon: 'Palette',
    keywords: ['color', 'hex', 'rgb', 'hsl', '颜色', '取色', '配色', 'ys'],
    action_payload: 'utools-hub',
    badge: '应用推荐'
  },
  {
    id: 'market-rec:port-kill',
    title: '端口占用查询与进程清理 (Port Killer)',
    subtitle: '应用推荐 · 快速查看 3000/8080 等本地端口占用 PID 并一键释放',
    category: 'plugin',
    icon: 'Terminal',
    keywords: ['port', 'kill', 'process', 'pid', '端口', '进程', 'dk'],
    action_payload: 'utools-hub',
    badge: '应用推荐'
  }
]

export async function queryLauncherItems(
  query: string,
  extraDynamicItems: Omit<SearchResultItem, 'score' | 'match_type'>[] = [],
  aliases: Record<string, string> = {},
  maxLimit = 14,
  showAppRecommendation = true
): Promise<SearchResultItem[]> {
  const q = query.trim().toLowerCase()
  const frecency = getFrecencyMap()
  const allItems = [
    ...CORE_PLUGIN_ITEMS,
    ...extraDynamicItems,
    ...SYSTEM_COMMAND_ITEMS,
    ...(showAppRecommendation && q ? MARKET_RECOMMENDATION_ITEMS : [])
  ]

  // Build reverse lookup for user custom aliases: itemId -> alias[]
  const itemAliasesMap: Record<string, string[]> = {}
  for (const [aliasKey, targetId] of Object.entries(aliases)) {
    const cleanAlias = aliasKey.trim().toLowerCase()
    if (!cleanAlias) continue
    if (!itemAliasesMap[targetId]) itemAliasesMap[targetId] = []
    itemAliasesMap[targetId].push(cleanAlias)
  }

  const results: SearchResultItem[] = []
  for (const item of allItems) {
    const frecencyBonus = (frecency[item.id] || 0) * 25
    const customAliases = itemAliasesMap[item.id] || []

    if (!q) {
      const base =
        item.category === 'plugin' ? 500 : item.category === 'app' ? 240 : 120
      results.push({
        ...item,
        score: base + frecencyBonus,
        match_type: customAliases.length > 0 ? `别名: ${customAliases[0]}` : 'default'
      })
      continue
    }

    let bestScore: number | null = null
    let matchType = 'fuzzy'

    // 0. Highest priority: Custom Alias Match
    if (customAliases.includes(q)) {
      bestScore = 1800
      matchType = `别名直达 (${q})`
    } else if (customAliases.some((a) => a.startsWith(q))) {
      bestScore = 1350
      matchType = '别名匹配'
    }

    const titleLower = item.title.toLowerCase()
    const py = getPinyinIndex(`${item.title} ${item.keywords.join(' ')}`)

    if (bestScore === null) {
      if (titleLower === q) {
        bestScore = 1200
        matchType = '精准匹配'
      } else if (titleLower.startsWith(q)) {
        bestScore = 950
        matchType = '前缀匹配'
      } else if (titleLower.includes(q)) {
        bestScore = 780
        matchType = '包含匹配'
      }
    }

    if (bestScore === null) {
      for (const kw of item.keywords) {
        const kwL = kw.toLowerCase()
        if (kwL === q) {
          bestScore = 1100
          matchType = '指令直达'
          break
        } else if (kwL.startsWith(q)) {
          bestScore = 880
          matchType = '指令前缀'
          break
        } else if (kwL.includes(q)) {
          bestScore = 700
          matchType = '关键词'
        }
      }
    }

    // 5. Character / Chinese Subsequence Match on Title & Keywords (only when showAppRecommendation is enabled)
    if (bestScore === null && showAppRecommendation) {
      const titleSubScore = subsequenceScore(titleLower, q)
      if (titleSubScore !== null) {
        bestScore = 540 + titleSubScore
        matchType = '智能推荐'
      } else {
        for (const kw of item.keywords) {
          const kwSubScore = subsequenceScore(kw.toLowerCase(), q)
          if (kwSubScore !== null) {
            bestScore = 510 + kwSubScore
            matchType = '智能推荐'
            break
          }
        }
      }
    }

    if (bestScore === null) {
      const initScore = subsequenceScore(py.initials, q)
      if (initScore !== null) {
        bestScore = 620 + initScore
        matchType = '拼音首字母'
      }
    }

    if (bestScore === null && showAppRecommendation) {
      const fullScore = subsequenceScore(py.full, q)
      if (fullScore !== null) {
        bestScore = 490 + fullScore
        matchType = '全拼推荐'
      }
    }

    if (bestScore !== null) {
      const catBonus = item.category === 'plugin' ? 60 : item.category === 'app' ? 30 : 0
      results.push({
        ...item,
        score: bestScore + catBonus + frecencyBonus,
        match_type: matchType
      })
    }
  }

  return results.sort((a, b) => b.score - a.score).slice(0, maxLimit)
}

export function installUtoolsGlobalShim(onCopyNotification?: (text: string) => void) {
  const shim = {
    onPluginEnter: (cb: (action: { code: string; type: string; payload: unknown }) => void) => {
      ;(window as unknown as Record<string, unknown>).__utools_enter_cb = cb
    },
    onPluginOut: (cb: () => void) => {
      ;(window as unknown as Record<string, unknown>).__utools_out_cb = cb
    },
    copyText: (text: string) => {
      navigator.clipboard.writeText(text)
      onCopyNotification?.(text)
      return true
    },
    dbStorage: {
      setItem: (key: string, value: unknown) => {
        localStorage.setItem(`utools:dbStorage:${key}`, JSON.stringify(value))
      },
      getItem: (key: string) => {
        const raw = localStorage.getItem(`utools:dbStorage:${key}`)
        return raw ? JSON.parse(raw) : null
      },
      removeItem: (key: string) => {
        localStorage.removeItem(`utools:dbStorage:${key}`)
      }
    },
    db: {
      put: (doc: { _id: string; data?: unknown; [k: string]: unknown }) => {
        const rev = `1-${Date.now().toString(16)}`
        const stored = { ...doc, _rev: rev }
        localStorage.setItem(`utools:db:${doc._id}`, JSON.stringify(stored))
        return { id: doc._id, ok: true, rev }
      },
      get: (id: string) => {
        const raw = localStorage.getItem(`utools:db:${id}`)
        return raw ? JSON.parse(raw) : null
      },
      remove: (idOrDoc: string | { _id: string }) => {
        const id = typeof idOrDoc === 'string' ? idOrDoc : idOrDoc._id
        localStorage.removeItem(`utools:db:${id}`)
        return { id, ok: true }
      }
    }
  }

  ;(window as unknown as Record<string, unknown>).utools = shim
  ;(window as unknown as Record<string, unknown>).rubick = shim
}

/**
 * Trigger interactive macOS crosshair screen region selection (`screencapture -i -x`),
 * automatically hiding and restoring the current OmniBox window so underlying QR codes are never obscured.
 */
export async function captureScreenRegionBase64(): Promise<string | null> {
  if (isTauriEnv()) {
    return await invoke<string | null>('capture_screen_region_base64')
  }
  return null
}

/**
 * Read PNG or TIFF image directly from the native macOS NSPasteboard as a data URL (`data:image/png;base64,...`).
 */
export async function readClipboardImageBase64(): Promise<string | null> {
  if (isTauriEnv()) {
    try {
      return await invoke<string | null>('read_clipboard_image_base64')
    } catch {
      return null
    }
  }
  return null
}
