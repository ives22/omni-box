<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import {
  type VaultEntry,
  type VaultEnvType,
  type VaultSubAccount,
  type VaultTemplateType,
  encryptVaultData,
  decryptVaultData,
  kvGet,
  kvPut
} from '@/bridge/backend'
import {
  EXCEL_COLUMN_OPTIONS,
  type ExcelColumnField,
  parseExcelTsvRows,
  detectHeaderAndMapping,
  buildVaultEntriesFromExcel
} from './excelImporter'
import IconRenderer from '@/components/IconRenderer.vue'

const store = useLauncherStore()

// Session-level memory cache so switching out to copy/paste and returning within auto-lock window doesn't require re-typing master password
interface SessionVaultCache {
  masterKey: string
  items: VaultEntry[]
  unlockedAt: number
}
const globalObj = window as unknown as { __omniboxVaultSession?: SessionVaultCache | null }

const DEFAULT_VAULT_ITEMS: VaultEntry[] = [
  {
    id: 'v-polardb-prod-lobby',
    title: 'PolarDB · lobby 核心库',
    username: 'luck_admin',
    password: 'K2mN8pQ4rS6tU1#Prod',
    url: 'pe-gs5925998y057042f1o.rwlb.singapore.rds.aliyuncs.com:3306',
    category: '数据库中间件',
    notes: '生产核心库实例（含管理员/业务/只读/备份多角色账号）',
    updatedAt: Date.now() - 3600000 * 4,
    templateType: 'database',
    env: 'PROD',
    componentType: 'PolarDB',
    moduleName: 'lobby',
    publicAddress: 'pe-gs5925998y057042f1o.rwlb.singapore.rds.aliyuncs.com:3306',
    privateAddress: 'pe-gs5925998y057042f1.rwlb.singapore.rds.aliyuncs.com:3306',
    subAccounts: [
      {
        id: 'sub-polar-1',
        role: '管理员',
        username: 'luck_admin',
        password: 'K2mN8pQ4rS6tU1#Prod',
        notes: 'DBA 架构变更与运维最高权限'
      },
      {
        id: 'sub-polar-2',
        role: '业务读写',
        username: 'api_prod',
        password: 'vL9$mP2#xR7@qW4z',
        notes: '生产 K8s API 服务连接使用'
      },
      {
        id: 'sub-polar-3',
        role: '只读账号',
        username: 'game_reader',
        password: 'cD5!fG6@hJ7#kL8',
        notes: '研发线上问题排查只读查询'
      },
      {
        id: 'sub-polar-4',
        role: '备份专用',
        username: 'backup',
        password: 'xY9#zW8$vU7!tS6',
        notes: '数据库定时冷备任务账号'
      }
    ]
  },
  {
    id: 'v-aliyun-ram-matrix',
    title: '阿里云国际站 RAM 与 API 凭证',
    username: 'jalen@5275617704155966.onaliyun.com',
    password: 'Ali#Ram_Jalen2026',
    url: 'https://signin-intl.aliyun.com/5275617704155966.onaliyun.com/login.htm',
    category: '云与API',
    notes: '统一管理 RAM 子账号、AccessKey (AK/SK) 与 ACR 镜像仓库密码',
    updatedAt: Date.now() - 3600000 * 10,
    templateType: 'cloud',
    env: 'PROD',
    subAccounts: [
      {
        id: 'sub-ali-1',
        role: 'CI/CD 构建',
        username: 'jalen@5275617704155966.onaliyun.com',
        password: 'Ali#Ram_Jalen2026',
        accessKeyId: 'LTAI5t8cBMnbS1pA9xYz7788',
        accessKeySecret: 'H51hFEnW9kLmN2pQ4rS6tU8vWxYz0192',
        extraSecret: 'ACR: Acr#Registry_2026',
        notes: 'Jenkins agent 构建推送镜像使用'
      },
      {
        id: 'sub-ali-2',
        role: 'OSS 存储',
        username: 'oss@5275617704155966.onaliyun.com',
        password: '(未开启控制台登录)',
        accessKeyId: 'LTAI5t6mH8n7aBcDeFgH3321',
        accessKeySecret: 'hC6fR7aH3jKlMnOpQrStUvWxYz987654',
        notes: 'OSS 静态资源桶读写，仅 SDK 调用'
      }
    ]
  },
  {
    id: 'v-redis-prod-lobby',
    title: 'Redis · lobby 缓存集群',
    username: 'default',
    password: 'Rds$fG7!hJ8@kL9#zX2cV4bN6m',
    url: 'r-gs5681732d9c92a4.redis.singapore.rds.aliyuncs.com:6379',
    category: '数据库中间件',
    notes: '生产核心 Redis 集群',
    updatedAt: Date.now() - 3600000 * 18,
    templateType: 'database',
    env: 'PROD',
    componentType: 'Redis',
    moduleName: 'lobby',
    publicAddress: 'r-gs5681732d9c92a4pd.redis.singapore.rds.aliyuncs.com:6379',
    privateAddress: 'r-gs5681732d9c92a4.redis.singapore.rds.aliyuncs.com:6379',
    subAccounts: [
      {
        id: 'sub-redis-1',
        role: '主节点密码',
        username: 'default',
        password: 'Rds$fG7!hJ8@kL9#zX2cV4bN6m',
        notes: '生产环境主库连接'
      }
    ]
  },
  {
    id: 'v-grafana-prod',
    title: 'Grafana 监控平台',
    username: 'admin',
    password: 'Gf#Prod_2026!Monitor_99',
    url: 'https://grafana.1001.api-prod.com/',
    category: '监控与后台',
    notes: '生产环境核心指标大盘',
    updatedAt: Date.now() - 3600000 * 24,
    templateType: 'general',
    env: 'PROD'
  },
  {
    id: 'v-grafana-dev',
    title: 'Grafana 监控平台',
    username: 'admin',
    password: 'admin_dev_2026',
    url: 'https://grafana.doji-dev.com/',
    category: '监控与后台',
    notes: '开发环境联调监控大盘',
    updatedAt: Date.now() - 3600000 * 30,
    templateType: 'general',
    env: 'DEV'
  },
  {
    id: 'v-vpn-jumpserver',
    title: '阿里云 VPN 跳板服务器',
    username: 'ops_user',
    password: 'Vpn#Jump_2026$Ops',
    url: '8.222.141.141',
    category: '基础设施',
    notes: '家里电信网络直连使用',
    updatedAt: Date.now() - 3600000 * 48,
    templateType: 'general',
    env: 'COMMON',
    extraSecret: 'root / qLwRr$Hpr89hMcm'
  }
]

const isUnlocked = ref(false)
const masterPasswordInput = ref('omnibox2026')
const currentMasterKey = ref('')
const unlockError = ref('')

const vaultItems = ref<VaultEntry[]>([])
const viewMode = ref<'table' | 'split'>('table')
const selectedCategory = ref('全部')
const selectedEnv = ref<'ALL' | VaultEnvType>('ALL')
const selectedId = ref<string>('')
const globalShowPasswords = ref(false)
const revealedSubIds = ref<Record<string, boolean>>({})
const showGeneratorBar = ref(false)

// Password Generator State
const genLength = ref(20)
const genUpper = ref(true)
const genNumbers = ref(true)
const genSymbols = ref(true)
const generatedPassword = ref('')

// Create / Edit Modal State
const showEditorModal = ref(false)
const editingEntryId = ref<string | null>(null)
const formTemplateType = ref<VaultTemplateType>('general')
const formTitle = ref('')
const formCategory = ref('云与API')
const formEnv = ref<VaultEnvType>('PROD')
const formComponentType = ref('PolarDB')
const formModuleName = ref('')
const formUrl = ref('')
const formPublicAddress = ref('')
const formPrivateAddress = ref('')
const formNotes = ref('')
const formSubAccounts = ref<VaultSubAccount[]>([])

// Delete Confirmation State
const pendingDeleteTarget = ref<{
  type: 'entry' | 'subAccount'
  entry: VaultEntry
  subAccount?: VaultSubAccount
} | null>(null)

// Excel Batch Paste Import Modal State
const showExcelModal = ref(false)
const rawExcelText = ref('')
const excelHasHeader = ref(true)
const excelAggregateSame = ref(true)
const excelDefaultCategory = ref('云与API')
const excelColMapping = ref<ExcelColumnField[]>([])

const categories = ['全部', '云与API', '数据库中间件', '监控与后台', '基础设施', '个人账号']

const envFilterOptions: { key: 'ALL' | VaultEnvType; label: string }[] = [
  { key: 'ALL', label: '全部环境' },
  { key: 'PROD', label: 'PROD 生产' },
  { key: 'PREV', label: 'PREV 预发' },
  { key: 'TEST', label: 'TEST 测试' },
  { key: 'DEV', label: 'DEV 开发' },
  { key: 'COMMON', label: '公共/通用' }
]

/**
 * Normalize any legacy or new VaultEntry so it always returns at least 1 effective account row.
 */
function getEffectiveAccounts(item: VaultEntry): VaultSubAccount[] {
  if (item.subAccounts && item.subAccounts.length > 0) {
    return item.subAccounts
  }
  return [
    {
      id: `${item.id}-main`,
      role: item.templateType === 'database' ? '默认账号' : '主账号',
      username: item.username || '',
      password: item.password || '',
      accessKeyId: item.accessKeyId,
      accessKeySecret: item.accessKeySecret,
      extraSecret: item.extraSecret,
      notes: item.notes
    }
  ]
}

function normalizeLegacyEntries(list: VaultEntry[]): VaultEntry[] {
  // If user only had the old 3 demo items, automatically upgrade to the rich DevOps demo items
  const oldDemoIds = new Set(['v-github-prod', 'v-aws-console', 'v-postgres-master'])
  if (
    list.length <= 3 &&
    list.every((it) => oldDemoIds.has(it.id) && !it.templateType && !it.subAccounts)
  ) {
    return JSON.parse(JSON.stringify(DEFAULT_VAULT_ITEMS))
  }

  return list.map((item) => {
    const env: VaultEnvType = item.env || (item.title.toLowerCase().includes('prod') || item.notes?.includes('生产') ? 'PROD' : 'COMMON')
    const templateType: VaultTemplateType =
      item.templateType ||
      (item.publicAddress || item.privateAddress ? 'database' : item.accessKeyId ? 'cloud' : 'general')
    const category =
      item.category === '研发与云'
        ? '云与API'
        : item.category === '内部系统'
          ? '监控与后台'
          : item.category || '云与API'

    return {
      ...item,
      env,
      templateType,
      category,
      subAccounts: getEffectiveAccounts(item)
    }
  })
}

const filteredItems = computed(() => {
  const q = store.subInputQuery.trim().toLowerCase()
  return vaultItems.value.filter((item) => {
    if (selectedCategory.value !== '全部' && item.category !== selectedCategory.value) {
      return false
    }
    if (selectedEnv.value !== 'ALL' && (item.env || 'COMMON') !== selectedEnv.value) {
      return false
    }
    if (!q) return true

    const subHit = getEffectiveAccounts(item).some(
      (sub) =>
        sub.username.toLowerCase().includes(q) ||
        sub.role.toLowerCase().includes(q) ||
        (sub.accessKeyId && sub.accessKeyId.toLowerCase().includes(q)) ||
        (sub.extraSecret && sub.extraSecret.toLowerCase().includes(q)) ||
        (sub.notes && sub.notes.toLowerCase().includes(q))
    )

    return (
      item.title.toLowerCase().includes(q) ||
      item.category.toLowerCase().includes(q) ||
      (item.env && item.env.toLowerCase().includes(q)) ||
      (item.componentType && item.componentType.toLowerCase().includes(q)) ||
      (item.moduleName && item.moduleName.toLowerCase().includes(q)) ||
      item.url.toLowerCase().includes(q) ||
      (item.publicAddress && item.publicAddress.toLowerCase().includes(q)) ||
      (item.privateAddress && item.privateAddress.toLowerCase().includes(q)) ||
      item.notes.toLowerCase().includes(q) ||
      subHit
    )
  })
})

const totalSubAccountCount = computed(() => {
  return filteredItems.value.reduce((sum, item) => sum + getEffectiveAccounts(item).length, 0)
})

const activeItem = computed(() => {
  return (
    filteredItems.value.find((i) => i.id === selectedId.value) ||
    filteredItems.value[0] ||
    null
  )
})

function envBadgeClass(env?: VaultEnvType): string {
  switch (env) {
    case 'PROD':
      return store.isDark
        ? 'bg-rose-500/20 text-rose-300 border-rose-500/40 font-bold'
        : 'bg-rose-100 text-rose-700 border-rose-300 font-bold'
    case 'PREV':
      return store.isDark
        ? 'bg-amber-500/20 text-amber-300 border-amber-500/35'
        : 'bg-amber-100 text-amber-700 border-amber-300'
    case 'TEST':
      return store.isDark
        ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/35'
        : 'bg-cyan-50 text-cyan-700 border-cyan-200'
    case 'DEV':
      return store.isDark
        ? 'bg-indigo-500/20 text-indigo-300 border-indigo-500/35'
        : 'bg-indigo-50 text-indigo-700 border-indigo-200'
    default:
      return store.isDark
        ? 'bg-slate-800 text-slate-300 border-slate-700'
        : 'bg-slate-100 text-slate-600 border-slate-200'
  }
}

function generateStrongPassword(): string {
  const lower = 'abcdefghijkmnopqrstuvwxyz'
  const upper = 'ABCDEFGHJKLMNPQRSTUVWXYZ'
  const nums = '23456789'
  const syms = '!@#$%^&*_+-='

  let pool = lower
  if (genUpper.value) pool += upper
  if (genNumbers.value) pool += nums
  if (genSymbols.value) pool += syms

  const arr = new Uint32Array(genLength.value)
  crypto.getRandomValues(arr)
  let out = ''
  for (let i = 0; i < genLength.value; i++) {
    out += pool[arr[i] % pool.length]
  }
  generatedPassword.value = out
  return out
}

function syncSessionCache() {
  if (!isUnlocked.value || !currentMasterKey.value) return
  globalObj.__omniboxVaultSession = {
    masterKey: currentMasterKey.value,
    items: JSON.parse(JSON.stringify(vaultItems.value)),
    unlockedAt: Date.now()
  }
}

async function handleUnlock() {
  unlockError.value = ''
  const pwd = masterPasswordInput.value.trim()
  if (!pwd) {
    unlockError.value = '请输入主密码'
    return
  }

  const encryptedPacket = await kvGet<string | null>('password-vault', 'cipher_blob', null)
  if (!encryptedPacket) {
    const packet = await encryptVaultData(pwd, JSON.stringify(DEFAULT_VAULT_ITEMS))
    await kvPut('password-vault', 'cipher_blob', packet)
    vaultItems.value = JSON.parse(JSON.stringify(DEFAULT_VAULT_ITEMS))
    currentMasterKey.value = pwd
    isUnlocked.value = true
    selectedId.value = vaultItems.value[0]?.id || ''
    syncSessionCache()
    store.showToast('密码本已使用 Argon2id + AES-256-GCM 初始化加密并解锁')
    return
  }

  try {
    const plainJson = await decryptVaultData(pwd, encryptedPacket)
    const parsed = JSON.parse(plainJson) as VaultEntry[]
    vaultItems.value = normalizeLegacyEntries(parsed)
    currentMasterKey.value = pwd
    isUnlocked.value = true
    if (vaultItems.value.length > 0) {
      selectedId.value = vaultItems.value[0].id
    }
    await persistVault()
    store.showToast('AES-256-GCM 密文校验通过，保险箱已解锁')
  } catch {
    unlockError.value = '主密码验证失败（默认演示密码为 omnibox2026）'
  }
}

async function persistVault() {
  if (!currentMasterKey.value) return
  syncSessionCache()
  const packet = await encryptVaultData(currentMasterKey.value, JSON.stringify(vaultItems.value))
  await kvPut('password-vault', 'cipher_blob', packet)
}

function lockVault() {
  isUnlocked.value = false
  currentMasterKey.value = ''
  globalShowPasswords.value = false
  revealedSubIds.value = {}
  globalObj.__omniboxVaultSession = null
  store.showToast('密码本已安全锁定，内存密钥已擦除', 'info')
}

function toggleRevealSub(subId: string) {
  revealedSubIds.value[subId] = !revealedSubIds.value[subId]
}

function isSubPasswordVisible(subId: string): boolean {
  return globalShowPasswords.value || !!revealedSubIds.value[subId]
}

async function copyField(text: string | undefined, label: string) {
  const clean = (text || '').trim()
  if (!clean) {
    store.showToast(`${label}为空`, 'warning')
    return
  }
  await navigator.clipboard.writeText(clean)
  syncSessionCache()
  store.showToast(`已复制 ${label}`)
}

function openExternalUrl(rawUrl: string) {
  const clean = rawUrl.trim()
  if (!clean) return
  const target = /^https?:\/\//i.test(clean) ? clean : `https://${clean}`
  window.open(target, '_blank', 'noopener,noreferrer')
}

// --- Create / Edit / Duplicate / Add Sub-Account ---
function createEmptySubAccount(index = 1): VaultSubAccount {
  return {
    id: `sub-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    role: index === 1 ? '管理员' : `子账号 #${index}`,
    username: '',
    password: generateStrongPassword(),
    accessKeyId: '',
    accessKeySecret: '',
    extraSecret: '',
    notes: ''
  }
}

function openCreateModal(presetTemplate?: VaultTemplateType) {
  editingEntryId.value = null
  formTemplateType.value =
    presetTemplate ||
    (selectedCategory.value === '数据库中间件'
      ? 'database'
      : selectedCategory.value === '云与API'
        ? 'cloud'
        : 'general')
  formTitle.value = ''
  formCategory.value =
    selectedCategory.value !== '全部'
      ? selectedCategory.value
      : formTemplateType.value === 'database'
        ? '数据库中间件'
        : formTemplateType.value === 'cloud'
          ? '云与API'
          : '监控与后台'
  formEnv.value = selectedEnv.value !== 'ALL' ? selectedEnv.value : 'PROD'
  formComponentType.value = 'PolarDB'
  formModuleName.value = ''
  formUrl.value = ''
  formPublicAddress.value = ''
  formPrivateAddress.value = ''
  formNotes.value = ''
  formSubAccounts.value = [createEmptySubAccount(1)]
  showEditorModal.value = true
}

function openEditModal(entry: VaultEntry, appendNewSubAccount = false) {
  editingEntryId.value = entry.id
  formTemplateType.value = entry.templateType || 'general'
  formTitle.value = entry.title
  formCategory.value = entry.category || '云与API'
  formEnv.value = entry.env || 'COMMON'
  formComponentType.value = entry.componentType || 'PolarDB'
  formModuleName.value = entry.moduleName || ''
  formUrl.value = entry.url || ''
  formPublicAddress.value = entry.publicAddress || ''
  formPrivateAddress.value = entry.privateAddress || ''
  formNotes.value = entry.notes || ''
  formSubAccounts.value = JSON.parse(JSON.stringify(getEffectiveAccounts(entry)))

  if (appendNewSubAccount) {
    formSubAccounts.value.push(createEmptySubAccount(formSubAccounts.value.length + 1))
  }
  showEditorModal.value = true
}

function duplicateEntryForNewEnv(entry: VaultEntry) {
  editingEntryId.value = null
  formTemplateType.value = entry.templateType || 'general'
  formTitle.value = entry.title
  formCategory.value = entry.category || '云与API'
  // Cycle environment for quick multi-env entry creation (e.g. DEV -> TEST -> PREV -> PROD)
  const nextEnvMap: Record<VaultEnvType, VaultEnvType> = {
    DEV: 'TEST',
    TEST: 'PREV',
    PREV: 'PROD',
    PROD: 'DEV',
    COMMON: 'PROD'
  }
  formEnv.value = nextEnvMap[entry.env || 'COMMON']
  formComponentType.value = entry.componentType || 'PolarDB'
  formModuleName.value = entry.moduleName || ''
  formUrl.value = entry.url || ''
  formPublicAddress.value = entry.publicAddress || ''
  formPrivateAddress.value = entry.privateAddress || ''
  formNotes.value = entry.notes || ''
  formSubAccounts.value = getEffectiveAccounts(entry).map((sub, i) => ({
    ...sub,
    id: `sub-${Date.now()}-${i}-${Math.random().toString(36).slice(2, 5)}`
  }))
  showEditorModal.value = true
  store.showToast(`已克隆「${entry.title}」，请修改环境或地址后保存`, 'info')
}

function addSubAccountRowInForm() {
  formSubAccounts.value.push(createEmptySubAccount(formSubAccounts.value.length + 1))
}

function removeSubAccountRowInForm(index: number) {
  if (formSubAccounts.value.length <= 1) {
    store.showToast('至少保留一个账号凭证行', 'warning')
    return
  }
  formSubAccounts.value.splice(index, 1)
}

function fillRandomPasswordToSub(index: number) {
  const pwd = generateStrongPassword()
  if (formSubAccounts.value[index]) {
    formSubAccounts.value[index].password = pwd
  }
}

function selectTemplateType(t: VaultTemplateType) {
  formTemplateType.value = t
  if (t === 'database') {
    formCategory.value = '数据库中间件'
  } else if (t === 'cloud') {
    formCategory.value = '云与API'
  }
}

async function saveEditorEntry() {
  let finalTitle = formTitle.value.trim()
  if (!finalTitle && formTemplateType.value === 'database') {
    finalTitle = formModuleName.value.trim()
      ? `${formComponentType.value} · ${formModuleName.value.trim()}`
      : formComponentType.value
  }
  if (!finalTitle) {
    store.showToast('请填写平台/系统或组件名称', 'warning')
    return
  }

  const cleanedSubs = formSubAccounts.value
    .map((s, idx) => ({
      id: s.id || `sub-${Date.now()}-${idx}`,
      role: (s.role || '').trim() || `账号 #${idx + 1}`,
      username: (s.username || '').trim(),
      password: (s.password || '').trim(),
      accessKeyId: (s.accessKeyId || '').trim() || undefined,
      accessKeySecret: (s.accessKeySecret || '').trim() || undefined,
      extraSecret: (s.extraSecret || '').trim() || undefined,
      notes: (s.notes || '').trim() || undefined
    }))
    .filter((s) => s.username || s.password || s.accessKeyId || s.extraSecret)

  if (cleanedSubs.length === 0) {
    store.showToast('请至少填写一组有效的账号或密码/密钥', 'warning')
    return
  }

  const primarySub = cleanedSubs[0]
  const primaryUrl =
    formTemplateType.value === 'database'
      ? formPublicAddress.value.trim() || formPrivateAddress.value.trim() || formUrl.value.trim()
      : formUrl.value.trim()

  const payload: VaultEntry = {
    id: editingEntryId.value || `v-${Date.now()}`,
    title: finalTitle,
    username: primarySub.username,
    password: primarySub.password,
    url: primaryUrl,
    category: formCategory.value,
    notes: formNotes.value.trim() || primarySub.notes || '',
    updatedAt: Date.now(),
    templateType: formTemplateType.value,
    env: formEnv.value,
    componentType: formTemplateType.value === 'database' ? formComponentType.value.trim() : undefined,
    moduleName: formModuleName.value.trim() || undefined,
    publicAddress: formTemplateType.value === 'database' ? formPublicAddress.value.trim() : undefined,
    privateAddress: formTemplateType.value === 'database' ? formPrivateAddress.value.trim() : undefined,
    accessKeyId: primarySub.accessKeyId,
    accessKeySecret: primarySub.accessKeySecret,
    extraSecret: primarySub.extraSecret,
    subAccounts: cleanedSubs
  }

  if (editingEntryId.value) {
    const idx = vaultItems.value.findIndex((i) => i.id === editingEntryId.value)
    if (idx !== -1) {
      vaultItems.value[idx] = payload
    }
    store.showToast(`已更新加密条目：${payload.title}`)
  } else {
    vaultItems.value.unshift(payload)
    store.showToast(`已加密写入保险箱：${payload.title}`)
  }

  selectedId.value = payload.id
  showEditorModal.value = false
  await persistVault()
}

// --- Delete Confirmation ---
function requestDeleteEntry(entry: VaultEntry) {
  pendingDeleteTarget.value = {
    type: 'entry',
    entry
  }
}

function requestDeleteSubAccount(entry: VaultEntry, sub: VaultSubAccount) {
  const subs = getEffectiveAccounts(entry)
  if (subs.length <= 1) {
    requestDeleteEntry(entry)
    return
  }
  pendingDeleteTarget.value = {
    type: 'subAccount',
    entry,
    subAccount: sub
  }
}

async function confirmDeleteTarget() {
  const target = pendingDeleteTarget.value
  if (!target) return

  if (target.type === 'entry') {
    vaultItems.value = vaultItems.value.filter((i) => i.id !== target.entry.id)
    if (selectedId.value === target.entry.id) {
      selectedId.value = vaultItems.value[0]?.id || ''
    }
    store.showToast(`已删除条目：${target.entry.title}`, 'info')
  } else if (target.type === 'subAccount' && target.subAccount) {
    const idx = vaultItems.value.findIndex((i) => i.id === target.entry.id)
    if (idx !== -1) {
      const updatedSubs = getEffectiveAccounts(vaultItems.value[idx]).filter(
        (s) => s.id !== target.subAccount!.id
      )
      vaultItems.value[idx] = {
        ...vaultItems.value[idx],
        username: updatedSubs[0]?.username || '',
        password: updatedSubs[0]?.password || '',
        subAccounts: updatedSubs,
        updatedAt: Date.now()
      }
      store.showToast(`已移除子账号：${target.subAccount.username || target.subAccount.role}`, 'info')
    }
  }

  pendingDeleteTarget.value = null
  await persistVault()
}

// --- Excel Paste Import & CSV Export ---
const parsedExcelRows = computed(() => parseExcelTsvRows(rawExcelText.value))

const excelPreviewEntries = computed(() => {
  if (parsedExcelRows.value.length === 0) return []
  return buildVaultEntriesFromExcel(
    parsedExcelRows.value,
    excelColMapping.value,
    excelHasHeader.value,
    excelDefaultCategory.value,
    excelAggregateSame.value
  )
})

function handleExcelTextChanged() {
  const rows = parsedExcelRows.value
  if (rows.length > 0) {
    const detected = detectHeaderAndMapping(rows)
    excelHasHeader.value = detected.hasHeader
    excelColMapping.value = detected.mapping
  } else {
    excelColMapping.value = []
  }
}

async function openExcelImportModal() {
  showExcelModal.value = true
  try {
    const clip = await navigator.clipboard.readText()
    if (clip && clip.includes('\t') && clip.includes('\n')) {
      rawExcelText.value = clip
      handleExcelTextChanged()
      store.showToast('已自动读取剪贴板中的 Excel 表格数据', 'info')
    }
  } catch {
    // user can paste manually
  }
}

function loadExcelSample(sampleType: 'aliyun' | 'env' | 'db') {
  if (sampleType === 'aliyun') {
    rawExcelText.value = [
      'type\taddr\tusername\tpassword\tAccessKey ID\tAccessKey Secret\tACR password\tnotes',
      'API\thttps://signin-intl.aliyun.com/5275617704155966.onaliyun.com/login.htm\tjalen@5275617704155966.onaliyun.com\tpfxv1!H!c4l0b1c1f(x)v&1(x)\tLTAI5t8cBMnbS1pA9xYz\tH51hFEnW9kLmN2pQ4rS6tU8vWxYz01\tluck#2026\tJenkins agent 使用',
      '\t\toss@5275617704155966.onaliyun.com\t\tLTAI5t6mH8n7aBcDeFgH\thC6fR7aH3jKlMnOpQrStUvWxYz9876\t\tOSS，未开启控制台'
    ].join('\n')
  } else if (sampleType === 'env') {
    rawExcelText.value = [
      '环境\t序号\t系统\taddress\t帐号\t密码\t备注',
      'dev\t1\tprometheus\thttps://prometheus.doji-dev.com\tadmin\tX6pL9mK2nB4vC8xZ\t开发指标采集',
      'test\t1\tgrafana\thttps://grafana.doji-test.com\tadmin\tQ2wE4rT6yU8iO0pA\t测试监控看板',
      'prod\t1\tgrafana\thttps://grafana.1001.api-prod.com\tadmin\tM9nB8vC7xZ6lK5jH\t生产监控大盘'
    ].join('\n')
  } else {
    rawExcelText.value = [
      '组件\t环境\t业务模块\t地址(Pub)\t地址(Private)\t用户\t密码\t备注',
      'PolarDB\tProd\tlobby\tpe-gs5925998y057042f1o.rwlb.singapore.rds.aliyuncs.com:3306\tpe-gs5925998y057042f1.rwlb.singapore.rds.aliyuncs.com:3306\tluck_admin\tK2mN8pQ4rS6tU1#Prod\t管理员',
      '\t\t\t\t\tgame_reader\tcD5!fG6@hJ7#kL8\t只读账号',
      '\t\t\t\t\tbackup\txY9#zW8$vU7!tS6\t数据库备份账号'
    ].join('\n')
  }
  handleExcelTextChanged()
}

async function confirmExcelBatchImport() {
  const toImport = excelPreviewEntries.value
  if (toImport.length === 0) {
    store.showToast('未解析出有效账号数据，请检查粘贴内容或列映射', 'warning')
    return
  }
  vaultItems.value = [...toImport, ...vaultItems.value]
  selectedId.value = toImport[0].id
  showExcelModal.value = false
  rawExcelText.value = ''
  await persistVault()
  const totalAccounts = toImport.reduce((acc, it) => acc + getEffectiveAccounts(it).length, 0)
  store.showToast(`成功加密导入 ${toImport.length} 个资源实例（共 ${totalAccounts} 个账号）`)
}

async function exportVaultAsTsv() {
  const header = [
    '环境',
    '分类',
    '平台/组件名',
    '业务模块',
    '通用地址/外网(Pub)',
    '内网地址(Private)',
    '角色',
    '账号/用户名',
    '密码',
    'AccessKey ID',
    'AccessKey Secret',
    '其他密钥/ACR',
    '备注'
  ].join('\t')

  const lines: string[] = [header]
  for (const item of filteredItems.value) {
    const subs = getEffectiveAccounts(item)
    subs.forEach((sub, idx) => {
      lines.push(
        [
          idx === 0 ? item.env || 'COMMON' : '',
          idx === 0 ? item.category : '',
          idx === 0 ? item.title : '',
          idx === 0 ? item.moduleName || '' : '',
          idx === 0 ? item.publicAddress || item.url || '' : '',
          idx === 0 ? item.privateAddress || '' : '',
          sub.role || '',
          sub.username || '',
          sub.password || '',
          sub.accessKeyId || '',
          sub.accessKeySecret || '',
          sub.extraSecret || '',
          sub.notes || (idx === 0 ? item.notes : '') || ''
        ].join('\t')
      )
    })
  }

  await navigator.clipboard.writeText(lines.join('\n'))
  store.showToast('已将当前筛选账密按 Excel 表格格式复制到剪贴板，可直接粘贴至 Excel！')
}

onMounted(() => {
  generateStrongPassword()
  // Check if session cache is still valid within auto-lock window
  const cached = globalObj.__omniboxVaultSession
  const autoLockMs = (store.userSettings.vaultAutoLockMinutes || 15) * 60 * 1000
  if (cached && Date.now() - cached.unlockedAt < autoLockMs) {
    currentMasterKey.value = cached.masterKey
    vaultItems.value = cached.items
    isUnlocked.value = true
    selectedId.value = vaultItems.value[0]?.id || ''
    cached.unlockedAt = Date.now()
  }
})
</script>

<template>
  <div
    class="flex-1 w-full min-w-0 h-full min-h-[500px] select-none flex flex-col relative overflow-hidden"
    :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
  >
    <!-- State 1: Locked Screen (Master Password Verification) -->
    <div
      v-if="!isUnlocked"
      class="flex-1 flex flex-col items-center justify-center px-6 text-center"
      :class="store.isDark ? 'bg-transparent' : 'bg-[#F8FAFC]'"
    >
      <div
        class="flex items-center justify-center w-14 h-14 mb-4 rounded-2xl border shadow-sm"
        :class="
          store.isDark
            ? 'bg-indigo-500/15 border-indigo-500/30 text-indigo-400'
            : 'bg-indigo-50 border-indigo-200/80 text-indigo-600'
        "
      >
        <IconRenderer name="Lock" :size="25" />
      </div>
      <h2 class="text-base font-bold tracking-tight" :class="store.isDark ? 'text-white' : 'text-slate-900'">
        研发与基础设施密码本已锁定
      </h2>
      <p class="mt-1 text-xs text-slate-400 max-w-md">
        采用 <code class="font-mono text-indigo-500">Argon2id</code> + <code class="font-mono text-indigo-500">AES-256-GCM</code> 本地加密，解锁后 {{ store.userSettings.vaultAutoLockMinutes || 15 }} 分钟内切换窗口免重复输入主密码
      </p>

      <form class="mt-5 w-full max-w-xs space-y-3" @submit.prevent="handleUnlock">
        <input
          v-model="masterPasswordInput"
          type="password"
          placeholder="输入主密码 (演示默认: omnibox2026)"
          aria-label="密码本主密码输入框"
          class="w-full px-3.5 py-2 rounded-xl text-sm font-mono border outline-none focus:ring-2 focus:ring-indigo-500/30 focus:border-indigo-500"
          :class="
            store.isDark
              ? 'bg-obsidian-950 border-slate-700 text-white'
              : 'bg-white border-slate-200/90 text-slate-900 shadow-2xs'
          "
        />
        <p v-if="unlockError" class="text-xs text-rose-500">{{ unlockError }}</p>
        <button type="submit" class="btn-primary w-full !py-2">
          <IconRenderer name="Unlock" :size="14" />
          <span>验证主密码并解锁保险箱</span>
        </button>
      </form>
    </div>

    <!-- State 2: Unlocked Vault Workspace -->
    <div v-else class="flex-1 w-full min-w-0 flex flex-col min-h-0 overflow-hidden">
      <!-- Single-Row Compact Control & Filter Bar -->
      <div
        class="px-3.5 py-2 border-b flex items-center justify-between gap-2 shrink-0"
        :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800/80' : 'bg-white border-slate-200/80'"
      >
        <!-- Left: Compact Dropdown Filters (Category + Environment) & Summary -->
        <div class="flex items-center gap-2 min-w-0">
          <!-- Category Dropdown -->
          <div class="relative flex items-center">
            <select
              v-model="selectedCategory"
              aria-label="按分类筛选密码本"
              class="h-7 pl-2.5 pr-6 rounded-lg text-xs font-medium border outline-none cursor-pointer transition-colors"
              :class="
                selectedCategory !== '全部'
                  ? store.isDark
                    ? 'bg-indigo-500/20 border-indigo-500/40 text-indigo-300'
                    : 'bg-indigo-50 border-indigo-300 text-indigo-700'
                  : store.isDark
                    ? 'bg-obsidian-900 border-slate-700/90 text-slate-200 hover:border-slate-600'
                    : 'bg-slate-100/90 border-slate-200/90 text-slate-700 hover:border-slate-300'
              "
            >
              <option v-for="cat in categories" :key="cat" :value="cat">
                {{ cat === '全部' ? '全部分类' : cat }}
              </option>
            </select>
          </div>

          <!-- Environment Dropdown -->
          <div class="relative flex items-center">
            <select
              v-model="selectedEnv"
              aria-label="按环境筛选密码本"
              class="h-7 pl-2.5 pr-6 rounded-lg text-xs font-medium border outline-none cursor-pointer transition-colors"
              :class="
                selectedEnv === 'PROD'
                  ? 'bg-rose-500/15 border-rose-500/40 text-rose-600 dark:text-rose-300 font-bold'
                  : selectedEnv !== 'ALL'
                    ? store.isDark
                      ? 'bg-indigo-500/20 border-indigo-500/40 text-indigo-300'
                      : 'bg-indigo-50 border-indigo-300 text-indigo-700'
                    : store.isDark
                      ? 'bg-obsidian-900 border-slate-700/90 text-slate-200 hover:border-slate-600'
                      : 'bg-slate-100/90 border-slate-200/90 text-slate-700 hover:border-slate-300'
              "
            >
              <option v-for="envOpt in envFilterOptions" :key="envOpt.key" :value="envOpt.key">
                {{ envOpt.label }}
              </option>
            </select>
          </div>

          <!-- Quick Reset if Filtered -->
          <button
            v-if="selectedCategory !== '全部' || selectedEnv !== 'ALL'"
            type="button"
            class="px-1.5 py-0.5 rounded text-[11px] text-slate-400 hover:text-indigo-500 cursor-pointer"
            @click="selectedCategory = '全部'; selectedEnv = 'ALL'"
          >
            重置
          </button>

          <span class="text-[11px] text-slate-400 hidden sm:inline truncate">
            {{ filteredItems.length }} 个资源 · {{ totalSubAccountCount }} 组账号
          </span>
        </div>

        <!-- Right: View Switcher & Batch Actions -->
        <div class="flex items-center gap-1.5 shrink-0">
          <!-- View Mode Toggle -->
          <div
            class="flex items-center p-0.5 rounded-lg border"
            :class="store.isDark ? 'bg-obsidian-900 border-slate-800' : 'bg-slate-100 border-slate-200/80'"
          >
            <button
              type="button"
              class="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] font-medium cursor-pointer transition-colors"
              :class="
                viewMode === 'table'
                  ? store.isDark
                    ? 'bg-indigo-500/25 text-indigo-300'
                    : 'bg-white text-indigo-600 shadow-2xs'
                  : 'text-slate-400 hover:text-slate-600'
              "
              title="类 Excel 高密度表格速览视图"
              @click="viewMode = 'table'"
            >
              <IconRenderer name="Table2" :size="13" />
              <span>表格视图</span>
            </button>
            <button
              type="button"
              class="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] font-medium cursor-pointer transition-colors"
              :class="
                viewMode === 'split'
                  ? store.isDark
                    ? 'bg-indigo-500/25 text-indigo-300'
                    : 'bg-white text-indigo-600 shadow-2xs'
                  : 'text-slate-400 hover:text-slate-600'
              "
              title="主从卡片详情视图"
              @click="viewMode = 'split'"
            >
              <IconRenderer name="Layers" :size="13" />
              <span>分栏详情</span>
            </button>
          </div>

          <button
            type="button"
            class="btn-secondary !py-1 !px-2 text-[11px]"
            :title="globalShowPasswords ? '全局隐藏所有密码明文' : '全局显示所有密码明文 (方便核对)'"
            @click="globalShowPasswords = !globalShowPasswords"
          >
            <IconRenderer :name="globalShowPasswords ? 'EyeOff' : 'Eye'" :size="13" />
            <span>{{ globalShowPasswords ? '隐藏明文' : '全局显密' }}</span>
          </button>

          <button
            type="button"
            class="btn-secondary !py-1 !px-2 text-[11px]"
            title="从 Excel 框选复制并粘贴批量导入"
            @click="openExcelImportModal"
          >
            <IconRenderer name="FileSpreadsheet" :size="13" />
            <span>Excel 导入</span>
          </button>

          <button
            type="button"
            class="btn-secondary !py-1 !px-1.5 text-[11px]"
            title="导出复制为 Excel 表格格式"
            @click="exportVaultAsTsv"
          >
            <IconRenderer name="Download" :size="13" />
          </button>

          <button
            type="button"
            class="btn-secondary !py-1 !px-1.5 text-[11px]"
            title="展开/收起随机密码生成器"
            @click="showGeneratorBar = !showGeneratorBar"
          >
            <IconRenderer name="Wand2" :size="13" />
          </button>

          <button
            type="button"
            class="btn-primary !py-1 !px-2.5 text-[11px]"
            @click="openCreateModal()"
          >
            <IconRenderer name="Plus" :size="13" />
            <span>新增资源</span>
          </button>

          <button
            type="button"
            title="立即锁定密码本并擦除内存密钥"
            aria-label="锁定密码本"
            class="btn-secondary !p-1.5 text-slate-400 hover:text-rose-500"
            @click="lockVault"
          >
            <IconRenderer name="Lock" :size="13" />
          </button>
        </div>
      </div>

      <!-- Collapsible Strong Password Generator Bar -->
      <div
        v-if="showGeneratorBar"
        class="px-4 py-2 border-b flex items-center justify-between gap-3 text-xs shrink-0"
        :class="store.isDark ? 'bg-indigo-950/20 border-slate-800' : 'bg-indigo-50/60 border-indigo-100'"
      >
        <div class="flex items-center gap-3">
          <span class="font-semibold text-indigo-500 flex items-center gap-1.5">
            <IconRenderer name="Sparkles" :size="13" />
            <span>高熵随机密码生成器 ({{ genLength }} 位)</span>
          </span>
          <label class="flex items-center gap-1 cursor-pointer text-[11px] text-slate-400">
            <input v-model="genUpper" type="checkbox" class="accent-indigo-600" @change="generateStrongPassword" />
            <span>A-Z</span>
          </label>
          <label class="flex items-center gap-1 cursor-pointer text-[11px] text-slate-400">
            <input v-model="genNumbers" type="checkbox" class="accent-indigo-600" @change="generateStrongPassword" />
            <span>0-9</span>
          </label>
          <label class="flex items-center gap-1 cursor-pointer text-[11px] text-slate-400">
            <input v-model="genSymbols" type="checkbox" class="accent-indigo-600" @change="generateStrongPassword" />
            <span>!@#$</span>
          </label>
          <input
            v-model.number="genLength"
            type="range"
            min="8"
            max="40"
            class="w-24 accent-indigo-600 cursor-pointer"
            @input="generateStrongPassword"
          />
        </div>

        <div class="flex items-center gap-2 flex-1 max-w-md">
          <div
            class="flex-1 px-3 py-1 rounded-lg font-mono text-xs border truncate select-text"
            :class="store.isDark ? 'bg-obsidian-950 border-slate-800 text-indigo-300' : 'bg-white border-indigo-200 text-indigo-700'"
          >
            {{ generatedPassword }}
          </div>
          <button type="button" class="btn-secondary !p-1" @click="generateStrongPassword">
            <IconRenderer name="RefreshCw" :size="13" />
          </button>
          <button
            type="button"
            class="btn-primary !py-1 !px-2.5 text-[11px]"
            @click="copyField(generatedPassword, '随机强密码')"
          >
            复制密码
          </button>
        </div>
      </div>

      <!-- MODE A: True Excel Merged-Cell (rowspan) Table with Sticky First/Last Columns & Auto-Width Horizontal Scroll -->
      <div
        v-if="viewMode === 'table'"
        class="flex-1 w-full overflow-x-auto overflow-y-auto min-h-0 vault-table-scroll"
      >
        <table class="w-max min-w-full border-separate border-spacing-0 text-left text-xs">
          <thead
            class="sticky top-0 z-30 text-[11px] font-semibold"
            :class="
              store.isDark
                ? 'bg-[#111726] text-slate-400'
                : 'bg-[#F1F5F9] text-slate-500'
            "
          >
            <tr>
              <!-- Col 1: Sticky Left Header -->
              <th
                class="sticky left-0 z-40 w-[156px] min-w-[156px] max-w-[156px] py-2 px-3 border-b border-r shadow-[2px_0_5px_-2px_rgba(0,0,0,0.12)]"
                :class="
                  store.isDark
                    ? 'bg-[#111726] border-slate-800'
                    : 'bg-[#F1F5F9] border-slate-200/90'
                "
              >
                环境 / 平台与组件
              </th>

              <!-- Col 2: Auto-Wrap Connection Address with Controlled Width -->
              <th
                class="w-[225px] min-w-[210px] max-w-[235px] py-2 px-3 border-b border-r whitespace-nowrap"
                :class="store.isDark ? 'bg-[#111726] border-slate-800' : 'bg-[#F1F5F9] border-slate-200/90'"
              >
                连接地址 (点击复制)
              </th>

              <!-- Col 3: Auto-Fit Role & Username -->
              <th
                class="py-2 px-3 border-b border-r whitespace-nowrap"
                :class="store.isDark ? 'bg-[#111726] border-slate-800' : 'bg-[#F1F5F9] border-slate-200/90'"
              >
                角色 / 登录账号
              </th>

              <!-- Col 4: Auto-Fit Password & Extended Keys -->
              <th
                class="py-2 px-3 border-b border-r whitespace-nowrap"
                :class="store.isDark ? 'bg-[#111726] border-slate-800' : 'bg-[#F1F5F9] border-slate-200/90'"
              >
                密码 / 扩展密钥 (AK·SK)
              </th>

              <!-- Col 5: Auto-Fit Notes -->
              <th
                class="py-2 px-3 border-b border-r whitespace-nowrap"
                :class="store.isDark ? 'bg-[#111726] border-slate-800' : 'bg-[#F1F5F9] border-slate-200/90'"
              >
                备注说明
              </th>

              <!-- Col 6: Sticky Right Header -->
              <th
                class="sticky right-0 z-40 w-[92px] min-w-[92px] py-2 px-2.5 text-right border-b border-l shadow-[-2px_0_5px_-2px_rgba(0,0,0,0.12)] whitespace-nowrap"
                :class="
                  store.isDark
                    ? 'bg-[#111726] border-slate-800'
                    : 'bg-[#F1F5F9] border-slate-200/90'
                "
              >
                维护操作
              </th>
            </tr>
          </thead>

          <!-- Empty State -->
          <tbody v-if="filteredItems.length === 0">
            <tr>
              <td colspan="6" class="py-16 text-center text-slate-400 text-xs">
                <div class="flex flex-col items-center gap-2">
                  <IconRenderer name="KeyRound" :size="24" class="opacity-40" />
                  <p>当前筛选条件下暂无密保条目</p>
                  <div class="flex items-center gap-2 mt-1">
                    <button type="button" class="btn-primary !py-1 !px-3 text-xs" @click="openCreateModal()">
                      + 新增资源条目
                    </button>
                    <button type="button" class="btn-secondary !py-1 !px-3 text-xs" @click="openExcelImportModal">
                      从 Excel 粘贴导入
                    </button>
                  </div>
                </div>
              </td>
            </tr>
          </tbody>

          <!-- Matrix Data Rows (Each item is a tbody group with native rowspan for merged cells) -->
          <template v-else>
            <tbody
              v-for="item in filteredItems"
              :key="item.id"
              class="group text-xs transition-colors"
              :class="[
                item.env === 'PROD'
                  ? store.isDark
                    ? 'bg-[#1a121b] hover:bg-[#221624]'
                    : 'bg-[#fff5f6] hover:bg-[#ffecef]'
                  : store.isDark
                    ? 'bg-[#0E131F] hover:bg-[#151c2c]'
                    : 'bg-[#F8FAFC] hover:bg-[#f1f5f9]'
              ]"
            >
              <tr
                v-for="(sub, subIdx) in getEffectiveAccounts(item)"
                :key="sub.id"
                class="align-middle"
              >
                <!-- Sticky Col 1: Environment + Platform Title (Merged via rowspan on first sub-row) -->
                <td
                  v-if="subIdx === 0"
                  :rowspan="getEffectiveAccounts(item).length"
                  class="sticky left-0 z-20 w-[156px] min-w-[156px] max-w-[156px] py-2.5 px-3 border-b border-r shadow-[2px_0_5px_-2px_rgba(0,0,0,0.1)] transition-colors align-middle"
                  :class="[
                    store.isDark ? 'border-slate-800/80' : 'border-slate-200/80',
                    item.env === 'PROD' ? 'border-l-[3px] border-l-rose-500' : '',
                    item.env === 'PROD'
                      ? store.isDark
                        ? 'bg-[#1a121b] group-hover:bg-[#221624]'
                        : 'bg-[#fff5f6] group-hover:bg-[#ffecef]'
                      : store.isDark
                        ? 'bg-[#0E131F] group-hover:bg-[#151c2c]'
                        : 'bg-[#F8FAFC] group-hover:bg-[#f1f5f9]'
                  ]"
                >
                  <div class="flex items-center gap-1.5 flex-wrap">
                    <span
                      class="px-1.5 py-0.5 rounded text-[10px] font-mono border leading-none"
                      :class="envBadgeClass(item.env)"
                    >
                      {{ item.env || 'COMMON' }}
                    </span>
                    <span
                      class="text-[10px] px-1.5 py-0.5 rounded truncate max-w-[80px]"
                      :class="store.isDark ? 'bg-slate-800 text-slate-400' : 'bg-slate-200/70 text-slate-600'"
                    >
                      {{ item.category }}
                    </span>
                  </div>
                  <div
                    class="font-normal text-[11px] mt-1.5 leading-snug break-words"
                    :class="store.isDark ? 'text-slate-200' : 'text-slate-700'"
                  >
                    {{ item.title }}
                  </div>
                </td>

                <!-- Middle Col 2: Endpoints / Dual Addresses (Merged via rowspan, Controlled Width + Auto Word-Wrap) -->
                <td
                  v-if="subIdx === 0"
                  :rowspan="getEffectiveAccounts(item).length"
                  class="w-[225px] min-w-[210px] max-w-[235px] py-2 px-2.5 border-b border-r font-mono text-[11px] align-middle"
                  :class="store.isDark ? 'border-slate-800/80' : 'border-slate-200/80'"
                >
                  <div class="space-y-1.5">
                    <template v-if="item.publicAddress || item.privateAddress">
                      <button
                        v-if="item.publicAddress"
                        type="button"
                        class="w-full flex items-start justify-between gap-1.5 px-2 py-1.5 rounded-lg border text-left cursor-pointer transition-colors"
                        :class="
                          store.isDark
                            ? 'bg-obsidian-950/70 border-slate-800 hover:border-sky-500/50 text-slate-300'
                            : 'bg-white border-slate-200/90 hover:border-sky-400 text-slate-700 shadow-2xs'
                        "
                        title="点击复制外网连接串 (Pub)"
                        @click="copyField(item.publicAddress, `${item.title} 外网地址(Pub)`)"
                      >
                        <span class="px-1 py-0.5 rounded text-[9px] font-sans font-semibold bg-sky-500/15 text-sky-500 shrink-0 mt-0.5 leading-none">
                          Pub
                        </span>
                        <span class="flex-1 min-w-0 break-all whitespace-normal leading-snug">
                          {{ item.publicAddress }}
                        </span>
                        <IconRenderer name="Copy" :size="11" class="text-slate-400 shrink-0 mt-0.5" />
                      </button>

                      <button
                        v-if="item.privateAddress"
                        type="button"
                        class="w-full flex items-start justify-between gap-1.5 px-2 py-1.5 rounded-lg border text-left cursor-pointer transition-colors"
                        :class="
                          store.isDark
                            ? 'bg-obsidian-950/70 border-slate-800 hover:border-emerald-500/50 text-slate-300'
                            : 'bg-white border-slate-200/90 hover:border-emerald-400 text-slate-700 shadow-2xs'
                        "
                        title="点击复制内网连接串 (Private)"
                        @click="copyField(item.privateAddress, `${item.title} 内网地址(Private)`)"
                      >
                        <span class="px-1 py-0.5 rounded text-[9px] font-sans font-semibold bg-emerald-500/15 text-emerald-500 shrink-0 mt-0.5 leading-none">
                          Priv
                        </span>
                        <span class="flex-1 min-w-0 break-all whitespace-normal leading-snug">
                          {{ item.privateAddress }}
                        </span>
                        <IconRenderer name="Copy" :size="11" class="text-slate-400 shrink-0 mt-0.5" />
                      </button>
                    </template>

                    <div
                      v-else-if="item.url"
                      class="w-full flex items-start justify-between gap-1.5 px-2 py-1.5 rounded-lg border"
                      :class="
                        store.isDark
                          ? 'bg-obsidian-950/70 border-slate-800 text-slate-300'
                          : 'bg-white border-slate-200/90 text-slate-700 shadow-2xs'
                      "
                    >
                      <span
                        class="flex-1 min-w-0 break-all whitespace-normal leading-snug cursor-pointer hover:text-indigo-500"
                        title="点击复制地址"
                        @click="copyField(item.url, `${item.title} 地址`)"
                      >
                        {{ item.url }}
                      </span>
                      <div class="flex items-center gap-1 shrink-0 mt-0.5">
                        <button
                          type="button"
                          class="p-0.5 text-slate-400 hover:text-indigo-500 cursor-pointer"
                          title="复制地址"
                          @click="copyField(item.url, `${item.title} 地址`)"
                        >
                          <IconRenderer name="Copy" :size="11" />
                        </button>
                        <button
                          v-if="/^https?:\/\//i.test(item.url)"
                          type="button"
                          class="p-0.5 text-slate-400 hover:text-indigo-500 cursor-pointer"
                          title="在浏览器中打开"
                          @click="openExternalUrl(item.url)"
                        >
                          <IconRenderer name="ExternalLink" :size="11" />
                        </button>
                      </div>
                    </div>
                    <span v-else class="text-slate-400 font-sans text-[11px] px-1">—</span>
                  </div>
                </td>

                <!-- Middle Col 3: Role + Full Username (Auto-Fit Full Text, Never Truncated) -->
                <td
                  class="px-3 py-2 border-b border-r whitespace-nowrap"
                  :class="store.isDark ? 'border-slate-800/80' : 'border-slate-200/80'"
                >
                  <div class="flex items-center gap-2 whitespace-nowrap">
                    <span
                      v-if="sub.role"
                      class="px-1.5 py-0.5 rounded text-[10px] font-medium shrink-0"
                      :class="
                        store.isDark
                          ? 'bg-slate-800 text-slate-300 border border-slate-700'
                          : 'bg-slate-200/70 text-slate-700'
                      "
                    >
                      {{ sub.role }}
                    </span>

                    <button
                      v-if="sub.username"
                      type="button"
                      class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded font-mono text-[11px] cursor-pointer transition-colors hover:bg-indigo-500/10 whitespace-nowrap"
                      :class="store.isDark ? 'text-slate-200 hover:text-indigo-300' : 'text-slate-800 hover:text-indigo-600'"
                      :title="`点击复制账号: ${sub.username}`"
                      @click="copyField(sub.username, `账号 (${sub.username})`)"
                    >
                      <span>{{ sub.username }}</span>
                      <IconRenderer name="Copy" :size="11" class="text-slate-400 shrink-0 opacity-75" />
                    </button>
                    <span v-else class="text-slate-400 text-[11px]">—</span>
                  </div>
                </td>

                <!-- Middle Col 4: Password + Multi-Line Stacked Extended Keys (AK/SK/ACR) -->
                <td
                  class="px-3 py-2 border-b border-r whitespace-nowrap"
                  :class="store.isDark ? 'border-slate-800/80' : 'border-slate-200/80'"
                >
                  <div class="flex flex-col items-start gap-1.5">
                    <!-- Line 1: Primary Password Pill -->
                    <div
                      v-if="sub.password"
                      class="inline-flex items-center rounded-md border overflow-hidden shrink-0"
                      :class="
                        store.isDark
                          ? 'bg-obsidian-950 border-indigo-500/30'
                          : 'bg-white border-indigo-200/90 shadow-2xs'
                      "
                    >
                      <button
                        type="button"
                        class="px-1.5 py-0.5 text-slate-400 hover:text-indigo-500 cursor-pointer border-r shrink-0"
                        :class="store.isDark ? 'border-slate-800' : 'border-indigo-100'"
                        :title="isSubPasswordVisible(sub.id) ? '隐藏密码' : '查看密码明文'"
                        @click="toggleRevealSub(sub.id)"
                      >
                        <IconRenderer :name="isSubPasswordVisible(sub.id) ? 'EyeOff' : 'Eye'" :size="11" />
                      </button>
                      <button
                        type="button"
                        class="px-2 py-0.5 font-mono text-[11px] flex items-center gap-1.5 cursor-pointer transition-colors whitespace-nowrap"
                        :class="
                          store.isDark
                            ? 'text-indigo-300 hover:bg-indigo-500/20'
                            : 'text-indigo-700 hover:bg-indigo-50'
                        "
                        :title="`点击直接复制密码 (${sub.username || item.title})`"
                        @click="copyField(sub.password, `${sub.username || item.title} 的密码`)"
                      >
                        <span>
                          {{ isSubPasswordVisible(sub.id) ? sub.password : '••••••••' }}
                        </span>
                        <IconRenderer name="Copy" :size="11" class="text-indigo-400 shrink-0 opacity-85" />
                      </button>
                    </div>

                    <!-- Line 2: Cloud AK / SK Row -->
                    <div
                      v-if="sub.accessKeyId || sub.accessKeySecret"
                      class="flex items-center gap-1.5 flex-wrap"
                    >
                      <button
                        v-if="sub.accessKeyId"
                        type="button"
                        class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded font-mono text-[10px] border cursor-pointer shrink-0"
                        :class="
                          store.isDark
                            ? 'bg-amber-500/15 border-amber-500/35 text-amber-300 hover:bg-amber-500/25'
                            : 'bg-amber-50 border-amber-200 text-amber-800 hover:bg-amber-100'
                        "
                        :title="`点击复制 AccessKey ID: ${sub.accessKeyId}`"
                        @click="copyField(sub.accessKeyId, 'AccessKey ID (AK)')"
                      >
                        <span class="font-sans font-semibold">AK:</span>
                        <span>{{ sub.accessKeyId }}</span>
                        <IconRenderer name="Copy" :size="10" />
                      </button>

                      <button
                        v-if="sub.accessKeySecret"
                        type="button"
                        class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded font-mono text-[10px] border cursor-pointer shrink-0"
                        :class="
                          store.isDark
                            ? 'bg-amber-500/20 border-amber-500/40 text-amber-200 hover:bg-amber-500/30'
                            : 'bg-amber-100/80 border-amber-300 text-amber-900 hover:bg-amber-200/80'
                        "
                        title="点击复制 AccessKey Secret (SK)"
                        @click="copyField(sub.accessKeySecret, 'AccessKey Secret (SK)')"
                      >
                        <span class="font-sans font-semibold">SK</span>
                        <IconRenderer name="Copy" :size="10" />
                      </button>
                    </div>

                    <!-- Line 3: Extra Secret (ACR / Root) Row -->
                    <button
                      v-if="sub.extraSecret"
                      type="button"
                      class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded font-mono text-[10px] border cursor-pointer shrink-0"
                      :class="
                        store.isDark
                          ? 'bg-purple-500/15 border-purple-500/35 text-purple-300 hover:bg-purple-500/25'
                          : 'bg-purple-50 border-purple-200 text-purple-700 hover:bg-purple-100'
                      "
                      :title="`点击复制附加密钥: ${sub.extraSecret}`"
                      @click="copyField(sub.extraSecret, '附加密钥/ACR密码')"
                    >
                      <span>{{ isSubPasswordVisible(sub.id) ? sub.extraSecret : '附加密钥/ACR' }}</span>
                      <IconRenderer name="Copy" :size="9" />
                    </button>
                  </div>
                </td>

                <!-- Middle Col 5: Dedicated Note Cell (Auto-Fit Full Text) -->
                <td
                  class="px-3 py-2 border-b border-r whitespace-nowrap"
                  :class="store.isDark ? 'border-slate-800/80' : 'border-slate-200/80'"
                >
                  <div class="flex items-center justify-between gap-2 whitespace-nowrap">
                    <span class="text-[11px] text-slate-400">
                      {{ sub.notes || (getEffectiveAccounts(item).length === 1 ? item.notes : '') || '—' }}
                    </span>
                    <button
                      v-if="getEffectiveAccounts(item).length > 1"
                      type="button"
                      class="opacity-0 group-hover:opacity-100 p-0.5 rounded text-slate-400 hover:text-rose-500 cursor-pointer shrink-0 transition-opacity"
                      title="仅移除此子账号"
                      @click="requestDeleteSubAccount(item, sub)"
                    >
                      <IconRenderer name="X" :size="12" />
                    </button>
                  </div>
                </td>

                <!-- Sticky Col 6: Instance Level Actions (Merged via rowspan on first sub-row, Always Fixed on Right) -->
                <td
                  v-if="subIdx === 0"
                  :rowspan="getEffectiveAccounts(item).length"
                  class="sticky right-0 z-20 w-[92px] min-w-[92px] py-2 px-2 text-right border-b border-l shadow-[-2px_0_5px_-2px_rgba(0,0,0,0.1)] transition-colors whitespace-nowrap align-middle"
                  :class="[
                    store.isDark ? 'border-slate-800/80' : 'border-slate-200/80',
                    item.env === 'PROD'
                      ? store.isDark
                        ? 'bg-[#1a121b] group-hover:bg-[#221624]'
                        : 'bg-[#fff5f6] group-hover:bg-[#ffecef]'
                      : store.isDark
                        ? 'bg-[#0E131F] group-hover:bg-[#151c2c]'
                        : 'bg-[#F8FAFC] group-hover:bg-[#f1f5f9]'
                  ]"
                >
                  <div class="inline-flex items-center justify-end gap-0.5">
                    <button
                      type="button"
                      class="p-1 rounded-lg text-slate-400 hover:text-indigo-500 hover:bg-indigo-500/10 cursor-pointer"
                      title="在此实例/平台下追加新角色子账号"
                      @click="openEditModal(item, true)"
                    >
                      <IconRenderer name="Plus" :size="13" />
                    </button>
                    <button
                      type="button"
                      class="p-1 rounded-lg text-slate-400 hover:text-indigo-500 hover:bg-indigo-500/10 cursor-pointer"
                      title="克隆为新环境条目 (如 dev -> test -> prod)"
                      @click="duplicateEntryForNewEnv(item)"
                    >
                      <IconRenderer name="CopyPlus" :size="13" />
                    </button>
                    <button
                      type="button"
                      class="p-1 rounded-lg text-slate-400 hover:text-indigo-500 hover:bg-indigo-500/10 cursor-pointer"
                      title="编辑此资源条目"
                      @click="openEditModal(item, false)"
                    >
                      <IconRenderer name="Pencil" :size="13" />
                    </button>
                    <button
                      type="button"
                      class="p-1 rounded-lg text-slate-400 hover:text-rose-500 hover:bg-rose-500/10 cursor-pointer"
                      title="删除此资源条目"
                      @click="requestDeleteEntry(item)"
                    >
                      <IconRenderer name="Trash2" :size="13" />
                    </button>
                  </div>
                </td>
              </tr>
            </tbody>
          </template>
        </table>
      </div>

      <!-- MODE B: Split Master-Detail Inspector View -->
      <div v-else class="flex-1 flex min-h-0 overflow-hidden">
        <aside
          class="w-64 border-r flex flex-col shrink-0 overflow-y-auto p-2 space-y-1"
          :class="store.isDark ? 'bg-obsidian-950/40 border-slate-800/80' : 'bg-slate-100/70 border-slate-200/80'"
        >
          <div
            v-for="item in filteredItems"
            :key="item.id"
            class="px-3 py-2.5 rounded-xl cursor-pointer transition-all"
            :class="[
              item.id === activeItem?.id
                ? store.isDark
                  ? 'bg-indigo-500/15 text-indigo-300 ring-1 ring-indigo-500/30'
                  : 'bg-white text-indigo-600 shadow-2xs ring-1 ring-slate-200/90'
                : store.isDark
                  ? 'text-slate-300 hover:bg-slate-800/50'
                  : 'text-slate-700 hover:bg-white/60',
              item.env === 'PROD' ? 'border-l-2 border-l-rose-500' : ''
            ]"
            @click="selectedId = item.id"
          >
            <div class="flex items-center justify-between gap-1">
              <span class="text-xs font-semibold truncate">{{ item.title }}</span>
              <span
                class="px-1.5 py-0.2 rounded text-[9px] font-mono border shrink-0"
                :class="envBadgeClass(item.env)"
              >
                {{ item.env || 'COMMON' }}
              </span>
            </div>
            <div class="flex items-center justify-between text-[11px] text-slate-400 font-mono mt-1">
              <span class="truncate">{{ getEffectiveAccounts(item)[0]?.username || item.url }}</span>
              <span v-if="getEffectiveAccounts(item).length > 1" class="text-[10px] font-sans text-indigo-400 shrink-0">
                {{ getEffectiveAccounts(item).length }} 个账号
              </span>
            </div>
          </div>
        </aside>

        <div class="flex-1 overflow-y-auto p-5">
          <template v-if="activeItem">
            <div
              class="flex items-start justify-between pb-4 border-b"
              :class="store.isDark ? 'border-slate-800' : 'border-slate-200/80'"
            >
              <div>
                <div class="flex items-center gap-2 flex-wrap">
                  <span
                    class="px-2 py-0.5 rounded text-[10px] font-mono border"
                    :class="envBadgeClass(activeItem.env)"
                  >
                    {{ activeItem.env || 'COMMON' }}
                  </span>
                  <h3 class="text-base font-bold" :class="store.isDark ? 'text-white' : 'text-slate-900'">
                    {{ activeItem.title }}
                  </h3>
                  <span
                    class="px-2 py-0.5 text-[10px] rounded-md border"
                    :class="
                      store.isDark
                        ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
                        : 'bg-indigo-50 text-indigo-600 border-indigo-200/80'
                    "
                  >
                    {{ activeItem.category }}
                  </span>
                </div>
                <p v-if="activeItem.notes" class="text-xs text-slate-400 mt-1.5">{{ activeItem.notes }}</p>
              </div>

              <div class="flex items-center gap-1.5">
                <button
                  type="button"
                  class="btn-secondary !py-1 !px-2.5 text-xs"
                  @click="openEditModal(activeItem, true)"
                >
                  <IconRenderer name="Plus" :size="13" />
                  <span>+子账号</span>
                </button>
                <button
                  type="button"
                  class="btn-secondary !py-1 !px-2.5 text-xs"
                  @click="openEditModal(activeItem, false)"
                >
                  <IconRenderer name="Pencil" :size="13" />
                  <span>编辑</span>
                </button>
                <button
                  type="button"
                  class="p-1.5 rounded-lg text-slate-400 hover:text-rose-500 hover:bg-rose-500/10 cursor-pointer"
                  @click="requestDeleteEntry(activeItem)"
                >
                  <IconRenderer name="Trash2" :size="15" />
                </button>
              </div>
            </div>

            <!-- Connection Endpoints Card -->
            <div
              v-if="activeItem.publicAddress || activeItem.privateAddress || activeItem.url"
              class="mt-4 p-3 rounded-xl border space-y-2 text-xs"
              :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800' : 'bg-white border-slate-200/80 shadow-2xs'"
            >
              <div class="text-[11px] font-semibold text-slate-400">连接地址 / Endpoints</div>
              <div v-if="activeItem.publicAddress" class="flex items-center justify-between gap-2">
                <div class="font-mono truncate">
                  <span class="text-sky-500 font-sans font-semibold mr-2">[外网 Pub]</span>
                  <span>{{ activeItem.publicAddress }}</span>
                </div>
                <button
                  type="button"
                  class="btn-secondary !py-1 !px-2 text-[11px] shrink-0"
                  @click="copyField(activeItem.publicAddress, '外网连接串(Pub)')"
                >
                  复制外网
                </button>
              </div>
              <div v-if="activeItem.privateAddress" class="flex items-center justify-between gap-2">
                <div class="font-mono truncate">
                  <span class="text-emerald-500 font-sans font-semibold mr-2">[内网 Priv]</span>
                  <span>{{ activeItem.privateAddress }}</span>
                </div>
                <button
                  type="button"
                  class="btn-secondary !py-1 !px-2 text-[11px] shrink-0"
                  @click="copyField(activeItem.privateAddress, '内网连接串(Private)')"
                >
                  复制内网
                </button>
              </div>
              <div
                v-if="!activeItem.publicAddress && !activeItem.privateAddress && activeItem.url"
                class="flex items-center justify-between gap-2"
              >
                <span class="font-mono truncate">{{ activeItem.url }}</span>
                <button
                  type="button"
                  class="btn-secondary !py-1 !px-2 text-[11px] shrink-0"
                  @click="copyField(activeItem.url, '访问地址')"
                >
                  复制地址
                </button>
              </div>
            </div>

            <!-- Sub-Accounts List -->
            <div class="mt-4 space-y-3">
              <div
                v-for="sub in getEffectiveAccounts(activeItem)"
                :key="sub.id"
                class="p-3.5 rounded-xl border space-y-2.5 text-xs"
                :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800' : 'bg-white border-slate-200/80 shadow-2xs'"
              >
                <div class="flex items-center justify-between">
                  <div class="flex items-center gap-2">
                    <span class="px-2 py-0.5 rounded bg-indigo-500/15 text-indigo-400 font-semibold text-[11px]">
                      {{ sub.role || '默认账号' }}
                    </span>
                    <span v-if="sub.notes" class="text-slate-400">{{ sub.notes }}</span>
                  </div>
                </div>

                <div class="grid grid-cols-2 gap-2.5">
                  <div
                    class="flex items-center justify-between p-2.5 rounded-lg border"
                    :class="store.isDark ? 'bg-obsidian-900/70 border-slate-800' : 'bg-slate-50 border-slate-200/70'"
                  >
                    <div class="min-w-0 pr-2">
                      <div class="text-[10px] text-slate-400">用户名 / 账号</div>
                      <div class="font-mono text-xs mt-0.5 truncate select-text">{{ sub.username || '—' }}</div>
                    </div>
                    <button
                      type="button"
                      class="btn-secondary !py-1 !px-2 text-[11px] shrink-0"
                      @click="copyField(sub.username, '账号')"
                    >
                      复制账号
                    </button>
                  </div>

                  <div
                    class="flex items-center justify-between p-2.5 rounded-lg border"
                    :class="store.isDark ? 'bg-obsidian-900/70 border-slate-800' : 'bg-slate-50 border-slate-200/70'"
                  >
                    <div class="min-w-0 pr-2">
                      <div class="text-[10px] text-slate-400">加密密码 (AES-256)</div>
                      <div class="font-mono text-xs mt-0.5 truncate text-indigo-400 select-text">
                        {{ isSubPasswordVisible(sub.id) ? sub.password : '••••••••••••' }}
                      </div>
                    </div>
                    <div class="flex items-center gap-1 shrink-0">
                      <button
                        type="button"
                        class="btn-secondary !p-1"
                        @click="toggleRevealSub(sub.id)"
                      >
                        <IconRenderer :name="isSubPasswordVisible(sub.id) ? 'EyeOff' : 'Eye'" :size="12" />
                      </button>
                      <button
                        type="button"
                        class="btn-primary !py-1 !px-2 text-[11px]"
                        @click="copyField(sub.password, '密码')"
                      >
                        复制密码
                      </button>
                    </div>
                  </div>
                </div>

                <div
                  v-if="sub.accessKeyId || sub.accessKeySecret || sub.extraSecret"
                  class="flex items-center gap-2 pt-1 flex-wrap"
                >
                  <button
                    v-if="sub.accessKeyId"
                    type="button"
                    class="btn-secondary !py-1 !px-2.5 text-[11px] font-mono"
                    @click="copyField(sub.accessKeyId, 'AccessKey ID')"
                  >
                    复制 AK ({{ sub.accessKeyId }})
                  </button>
                  <button
                    v-if="sub.accessKeySecret"
                    type="button"
                    class="btn-secondary !py-1 !px-2.5 text-[11px]"
                    @click="copyField(sub.accessKeySecret, 'AccessKey Secret')"
                  >
                    复制 AccessKey Secret (SK)
                  </button>
                  <button
                    v-if="sub.extraSecret"
                    type="button"
                    class="btn-secondary !py-1 !px-2.5 text-[11px]"
                    @click="copyField(sub.extraSecret, '附加密钥/ACR密码')"
                  >
                    复制附加密钥 ({{ sub.extraSecret }})
                  </button>
                </div>
              </div>
            </div>
          </template>
        </div>
      </div>
    </div>

    <!-- MODAL 1: Scenario-Aware Create / Edit Entry Modal -->
    <div
      v-if="showEditorModal"
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-xs"
      @click.self="showEditorModal = false"
    >
      <div
        class="w-full max-w-2xl max-h-[88vh] flex flex-col rounded-2xl border shadow-2xl overflow-hidden"
        :class="store.isDark ? 'bg-[#111726] border-slate-700/80 text-white' : 'bg-white border-slate-200 text-slate-900'"
      >
        <!-- Modal Header -->
        <div
          class="px-5 py-3.5 border-b flex items-center justify-between shrink-0"
          :class="store.isDark ? 'border-slate-800' : 'border-slate-200/80'"
        >
          <div class="flex items-center gap-2">
            <IconRenderer name="KeyRound" :size="16" class="text-indigo-500" />
            <h3 class="text-sm font-bold">
              {{ editingEntryId ? '编辑加密资源与账号矩阵' : '新增加密资源与账号矩阵' }}
            </h3>
          </div>
          <button
            type="button"
            class="p-1 rounded-lg text-slate-400 hover:text-slate-600 cursor-pointer"
            @click="showEditorModal = false"
          >
            <IconRenderer name="X" :size="16" />
          </button>
        </div>

        <!-- Modal Body -->
        <div class="p-5 overflow-y-auto space-y-4 text-xs">
          <!-- Scenario Template Selector -->
          <div>
            <label class="block text-slate-400 mb-1.5 font-medium">选择资产场景模板（自动适配专属字段）</label>
            <div class="grid grid-cols-3 gap-2">
              <button
                type="button"
                class="p-2.5 rounded-xl border text-left cursor-pointer transition-all"
                :class="
                  formTemplateType === 'general'
                    ? 'border-indigo-500 bg-indigo-500/10 text-indigo-400 font-semibold'
                    : store.isDark
                      ? 'border-slate-800 bg-obsidian-950/60 text-slate-400'
                      : 'border-slate-200 bg-slate-50 text-slate-600'
                "
                @click="selectTemplateType('general')"
              >
                <div class="flex items-center gap-1.5 text-xs">
                  <IconRenderer name="Globe" :size="14" />
                  <span>通用平台 / 多环境后台</span>
                </div>
                <p class="text-[10px] opacity-75 mt-0.5">Grafana / VPN / GitLab / 服务器</p>
              </button>

              <button
                type="button"
                class="p-2.5 rounded-xl border text-left cursor-pointer transition-all"
                :class="
                  formTemplateType === 'cloud'
                    ? 'border-indigo-500 bg-indigo-500/10 text-indigo-400 font-semibold'
                    : store.isDark
                      ? 'border-slate-800 bg-obsidian-950/60 text-slate-400'
                      : 'border-slate-200 bg-slate-50 text-slate-600'
                "
                @click="selectTemplateType('cloud')"
              >
                <div class="flex items-center gap-1.5 text-xs">
                  <IconRenderer name="Cloud" :size="14" />
                  <span>云厂商与 API 凭证</span>
                </div>
                <p class="text-[10px] opacity-75 mt-0.5">支持 RAM 子账号 + AK/SK + ACR</p>
              </button>

              <button
                type="button"
                class="p-2.5 rounded-xl border text-left cursor-pointer transition-all"
                :class="
                  formTemplateType === 'database'
                    ? 'border-indigo-500 bg-indigo-500/10 text-indigo-400 font-semibold'
                    : store.isDark
                      ? 'border-slate-800 bg-obsidian-950/60 text-slate-400'
                      : 'border-slate-200 bg-slate-50 text-slate-600'
                "
                @click="selectTemplateType('database')"
              >
                <div class="flex items-center gap-1.5 text-xs">
                  <IconRenderer name="Database" :size="14" />
                  <span>数据库与中间件实例</span>
                </div>
                <p class="text-[10px] opacity-75 mt-0.5">外网Pub/内网Priv + 多角色账号</p>
              </button>
            </div>
          </div>

          <!-- Instance Common Fields -->
          <div class="grid grid-cols-12 gap-3">
            <div class="col-span-3">
              <label class="block text-slate-400 mb-1">所属环境</label>
              <select
                v-model="formEnv"
                class="w-full px-2.5 py-2 rounded-xl border outline-none font-mono"
                :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200'"
              >
                <option value="PROD">PROD (生产警示)</option>
                <option value="PREV">PREV (预发)</option>
                <option value="TEST">TEST (测试)</option>
                <option value="DEV">DEV (开发)</option>
                <option value="COMMON">COMMON (通用)</option>
              </select>
            </div>

            <div class="col-span-3">
              <label class="block text-slate-400 mb-1">所属分类</label>
              <select
                v-model="formCategory"
                class="w-full px-2.5 py-2 rounded-xl border outline-none"
                :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200'"
              >
                <option v-for="c in categories.slice(1)" :key="c" :value="c">{{ c }}</option>
              </select>
            </div>

            <template v-if="formTemplateType === 'database'">
              <div class="col-span-3">
                <label class="block text-slate-400 mb-1">组件类型</label>
                <input
                  v-model="formComponentType"
                  type="text"
                  placeholder="PolarDB / Redis / MongoDB"
                  class="w-full px-3 py-2 rounded-xl border outline-none"
                  :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200'"
                />
              </div>
              <div class="col-span-3">
                <label class="block text-slate-400 mb-1">业务模块 / 集群</label>
                <input
                  v-model="formModuleName"
                  type="text"
                  placeholder="如：lobby / opx"
                  class="w-full px-3 py-2 rounded-xl border outline-none"
                  :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200'"
                />
              </div>
            </template>

            <div :class="formTemplateType === 'database' ? 'col-span-12' : 'col-span-6'">
              <label class="block text-slate-400 mb-1">资源显示标题</label>
              <input
                v-model="formTitle"
                type="text"
                :placeholder="
                  formTemplateType === 'database'
                    ? '留空将自动按「组件 · 业务模块」生成，如 PolarDB · lobby'
                    : '如：阿里云国际站 RAM / Grafana 监控平台'
                "
                class="w-full px-3 py-2 rounded-xl border outline-none"
                :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200'"
              />
            </div>

            <!-- Endpoints -->
            <template v-if="formTemplateType === 'database'">
              <div class="col-span-6">
                <label class="block text-sky-400 mb-1 font-medium">外网地址 (Pub 连接串/Endpoint)</label>
                <input
                  v-model="formPublicAddress"
                  type="text"
                  placeholder="pe-xxx.rwlb.rds.aliyuncs.com:3306"
                  class="w-full px-3 py-2 rounded-xl font-mono border outline-none"
                  :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200'"
                />
              </div>
              <div class="col-span-6">
                <label class="block text-emerald-400 mb-1 font-medium">内网地址 (Private 连接串/URI)</label>
                <input
                  v-model="formPrivateAddress"
                  type="text"
                  placeholder="pe-xxx.rwlb.rds.aliyuncs.com:3306 或 mongodb://..."
                  class="w-full px-3 py-2 rounded-xl font-mono border outline-none"
                  :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200'"
                />
              </div>
            </template>
            <div v-else class="col-span-12">
              <label class="block text-slate-400 mb-1">登录网址 URL / 服务器 IP</label>
              <input
                v-model="formUrl"
                type="text"
                placeholder="https://... 或 8.222.141.141"
                class="w-full px-3 py-2 rounded-xl font-mono border outline-none"
                :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200'"
              />
            </div>
          </div>

          <!-- Multi-Account Matrix List inside this Resource -->
          <div class="pt-2">
            <div class="flex items-center justify-between mb-2">
              <span class="font-bold text-xs flex items-center gap-1.5">
                <span>挂载账号列表 ({{ formSubAccounts.length }} 个角色/子账号共用上述地址)</span>
              </span>
              <button
                type="button"
                class="btn-secondary !py-1 !px-2.5 text-[11px]"
                @click="addSubAccountRowInForm"
              >
                <IconRenderer name="Plus" :size="12" />
                <span>添加子账号 (如只读/备份/RAM)</span>
              </button>
            </div>

            <div class="space-y-2.5">
              <div
                v-for="(sub, idx) in formSubAccounts"
                :key="sub.id"
                class="p-3 rounded-xl border space-y-2"
                :class="store.isDark ? 'bg-obsidian-950/80 border-slate-800' : 'bg-slate-50/80 border-slate-200/90'"
              >
                <div class="grid grid-cols-12 gap-2 items-center">
                  <div class="col-span-3">
                    <label class="block text-[10px] text-slate-400 mb-0.5">角色 / 标识</label>
                    <input
                      v-model="sub.role"
                      type="text"
                      placeholder="管理员 / 只读 / CI"
                      class="w-full px-2.5 py-1.5 rounded-lg border outline-none"
                      :class="store.isDark ? 'bg-obsidian-900 border-slate-700 text-white' : 'bg-white border-slate-200'"
                    />
                  </div>
                  <div class="col-span-4">
                    <label class="block text-[10px] text-slate-400 mb-0.5">登录账号 / 用户名</label>
                    <input
                      v-model="sub.username"
                      type="text"
                      placeholder="username / email"
                      class="w-full px-2.5 py-1.5 rounded-lg font-mono border outline-none"
                      :class="store.isDark ? 'bg-obsidian-900 border-slate-700 text-white' : 'bg-white border-slate-200'"
                    />
                  </div>
                  <div class="col-span-5">
                    <label class="block text-[10px] text-slate-400 mb-0.5">密码原文</label>
                    <div class="flex items-center gap-1">
                      <input
                        v-model="sub.password"
                        type="text"
                        placeholder="输入或右侧随机生成"
                        class="w-full px-2.5 py-1.5 rounded-lg font-mono border outline-none text-indigo-400"
                        :class="store.isDark ? 'bg-obsidian-900 border-slate-700' : 'bg-white border-slate-200'"
                      />
                      <button
                        type="button"
                        class="btn-secondary !p-1.5 shrink-0"
                        title="生成随机强密码"
                        @click="fillRandomPasswordToSub(idx)"
                      >
                        <IconRenderer name="RefreshCw" :size="12" />
                      </button>
                      <button
                        v-if="formSubAccounts.length > 1"
                        type="button"
                        class="p-1.5 rounded-lg text-slate-400 hover:text-rose-500 cursor-pointer shrink-0"
                        title="移除此子账号行"
                        @click="removeSubAccountRowInForm(idx)"
                      >
                        <IconRenderer name="Trash2" :size="13" />
                      </button>
                    </div>
                  </div>
                </div>

                <!-- Cloud AK/SK & Extra Secret Row -->
                <div class="grid grid-cols-12 gap-2">
                  <template v-if="formTemplateType === 'cloud'">
                    <div class="col-span-4">
                      <input
                        v-model="sub.accessKeyId"
                        type="text"
                        placeholder="AccessKey ID (AK，选填)"
                        class="w-full px-2.5 py-1 rounded-lg font-mono text-[11px] border outline-none"
                        :class="store.isDark ? 'bg-obsidian-900 border-slate-800 text-amber-300' : 'bg-white border-slate-200'"
                      />
                    </div>
                    <div class="col-span-4">
                      <input
                        v-model="sub.accessKeySecret"
                        type="text"
                        placeholder="AccessKey Secret (SK，选填)"
                        class="w-full px-2.5 py-1 rounded-lg font-mono text-[11px] border outline-none"
                        :class="store.isDark ? 'bg-obsidian-900 border-slate-800 text-amber-300' : 'bg-white border-slate-200'"
                      />
                    </div>
                  </template>
                  <div :class="formTemplateType === 'cloud' ? 'col-span-4' : 'col-span-5'">
                    <input
                      v-model="sub.extraSecret"
                      type="text"
                      :placeholder="
                        formTemplateType === 'cloud'
                          ? 'ACR 镜像密码 / 其他密钥 (选填)'
                          : '其他 / 二级 root 密码 (选填)'
                      "
                      class="w-full px-2.5 py-1 rounded-lg font-mono text-[11px] border outline-none"
                      :class="store.isDark ? 'bg-obsidian-900 border-slate-800 text-purple-300' : 'bg-white border-slate-200'"
                    />
                  </div>
                  <div :class="formTemplateType === 'cloud' ? 'col-span-12' : 'col-span-7'">
                    <input
                      v-model="sub.notes"
                      type="text"
                      placeholder="账号用途备注（如：Jenkins agent 使用 / 测试人员只读）"
                      class="w-full px-2.5 py-1 rounded-lg text-[11px] border outline-none"
                      :class="store.isDark ? 'bg-obsidian-900 border-slate-800 text-slate-300' : 'bg-white border-slate-200'"
                    />
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Modal Footer -->
        <div
          class="px-5 py-3 border-t flex items-center justify-end gap-2 shrink-0"
          :class="store.isDark ? 'border-slate-800 bg-obsidian-950/50' : 'border-slate-200/80 bg-slate-50'"
        >
          <button type="button" class="btn-secondary !py-1.5 !px-3.5 text-xs" @click="showEditorModal = false">
            取消
          </button>
          <button type="button" class="btn-primary !py-1.5 !px-4 text-xs" @click="saveEditorEntry">
            <IconRenderer name="Check" :size="14" />
            <span>AES-256-GCM 加密保存</span>
          </button>
        </div>
      </div>
    </div>

    <!-- MODAL 2: Excel / TSV Smart Batch Paste Importer -->
    <div
      v-if="showExcelModal"
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-xs"
      @click.self="showExcelModal = false"
    >
      <div
        class="w-full max-w-3xl max-h-[90vh] flex flex-col rounded-2xl border shadow-2xl overflow-hidden"
        :class="store.isDark ? 'bg-[#111726] border-slate-700/80 text-white' : 'bg-white border-slate-200 text-slate-900'"
      >
        <div
          class="px-5 py-3.5 border-b flex items-center justify-between shrink-0"
          :class="store.isDark ? 'border-slate-800' : 'border-slate-200/80'"
        >
          <div class="flex items-center gap-2">
            <IconRenderer name="FileSpreadsheet" :size="16" class="text-emerald-500" />
            <div>
              <h3 class="text-sm font-bold">从 Excel 框选粘贴智能导入</h3>
              <p class="text-[11px] text-slate-400">
                直接在 Excel 中框选任意区域复制 (<code class="font-mono">Cmd+C</code>)，粘贴到下方即可自动识别合并单元格与多账号结构
              </p>
            </div>
          </div>
          <button
            type="button"
            class="p-1 rounded-lg text-slate-400 hover:text-slate-600 cursor-pointer"
            @click="showExcelModal = false"
          >
            <IconRenderer name="X" :size="16" />
          </button>
        </div>

        <div class="p-5 overflow-y-auto space-y-4 text-xs">
          <!-- Quick Sample Presets -->
          <div class="flex items-center justify-between gap-2 flex-wrap">
            <div class="flex items-center gap-1.5">
              <span class="text-slate-400">一键载入您的 Excel 格式示例：</span>
              <button
                type="button"
                class="btn-secondary !py-1 !px-2 text-[11px]"
                @click="loadExcelSample('db')"
              >
                示例 1：PolarDB 双地址合并单元格
              </button>
              <button
                type="button"
                class="btn-secondary !py-1 !px-2 text-[11px]"
                @click="loadExcelSample('aliyun')"
              >
                示例 2：阿里云 RAM + AK/SK 表
              </button>
              <button
                type="button"
                class="btn-secondary !py-1 !px-2 text-[11px]"
                @click="loadExcelSample('env')"
              >
                示例 3：dev/test/prod 多环境监控表
              </button>
            </div>

            <div class="flex items-center gap-3">
              <label class="flex items-center gap-1.5 cursor-pointer">
                <input v-model="excelHasHeader" type="checkbox" class="accent-indigo-600" />
                <span>首行为表头</span>
              </label>
              <label class="flex items-center gap-1.5 cursor-pointer">
                <input v-model="excelAggregateSame" type="checkbox" class="accent-indigo-600" />
                <span>合并同名实例子账号</span>
              </label>
            </div>
          </div>

          <textarea
            v-model="rawExcelText"
            rows="5"
            placeholder="在此处按 Cmd + V 粘贴从 Excel 复制的单元格内容（支持 Tab 分隔与合并单元格留空行自动继承）..."
            class="w-full p-3 rounded-xl font-mono text-xs border outline-none focus:ring-2 focus:ring-indigo-500/30"
            :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-slate-200' : 'bg-slate-50 border-slate-200 text-slate-800'"
            @input="handleExcelTextChanged"
          />

          <!-- Column Mapping Bar -->
          <div v-if="parsedExcelRows.length > 0" class="space-y-2">
            <div class="font-semibold text-slate-400">
              列字段映射识别（共检测到 {{ parsedExcelRows[0]?.length || 0 }} 列，可手动微调）
            </div>
            <div class="grid grid-cols-4 gap-2">
              <div
                v-for="(_, colIdx) in parsedExcelRows[0]"
                :key="colIdx"
                class="p-2 rounded-lg border"
                :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800' : 'bg-slate-50 border-slate-200/80'"
              >
                <div class="text-[10px] text-slate-400 truncate mb-1">
                  第 {{ colIdx + 1 }} 列 ({{ parsedExcelRows[0][colIdx] || '空表头' }})
                </div>
                <select
                  v-model="excelColMapping[colIdx]"
                  class="w-full px-2 py-1 rounded border text-[11px] outline-none"
                  :class="store.isDark ? 'bg-obsidian-900 border-slate-700 text-indigo-300' : 'bg-white border-slate-200 text-indigo-700'"
                >
                  <option v-for="opt in EXCEL_COLUMN_OPTIONS" :key="opt.field" :value="opt.field">
                    {{ opt.label }}
                  </option>
                </select>
              </div>
            </div>
          </div>

          <!-- Parsed Preview -->
          <div v-if="excelPreviewEntries.length > 0" class="space-y-2">
            <div class="font-semibold text-emerald-500">
              解析预览：将生成 {{ excelPreviewEntries.length }} 个资源实例（包含
              {{ excelPreviewEntries.reduce((s, e) => s + getEffectiveAccounts(e).length, 0) }} 组子账号凭证）
            </div>
            <div
              class="max-h-44 overflow-y-auto rounded-xl border divide-y"
              :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800 divide-slate-800' : 'bg-white border-slate-200 divide-slate-200'"
            >
              <div
                v-for="prev in excelPreviewEntries"
                :key="prev.id"
                class="px-3 py-2 flex items-center justify-between gap-2"
              >
                <div class="flex items-center gap-2 min-w-0">
                  <span class="px-1.5 py-0.2 rounded text-[10px] font-mono border" :class="envBadgeClass(prev.env)">
                    {{ prev.env }}
                  </span>
                  <span class="font-bold truncate">{{ prev.title }}</span>
                  <span class="text-[11px] text-slate-400 font-mono truncate">
                    {{ prev.publicAddress || prev.url }}
                  </span>
                </div>
                <span class="text-[11px] text-indigo-400 shrink-0">
                  {{ getEffectiveAccounts(prev).length }} 个账号 ({{
                    getEffectiveAccounts(prev)
                      .map((a) => a.username || a.role)
                      .join(', ')
                  }})
                </span>
              </div>
            </div>
          </div>
        </div>

        <div
          class="px-5 py-3 border-t flex items-center justify-end gap-2 shrink-0"
          :class="store.isDark ? 'border-slate-800 bg-obsidian-950/50' : 'border-slate-200/80 bg-slate-50'"
        >
          <button type="button" class="btn-secondary !py-1.5 !px-3.5 text-xs" @click="showExcelModal = false">
            取消
          </button>
          <button
            type="button"
            class="btn-primary !py-1.5 !px-4 text-xs"
            :disabled="excelPreviewEntries.length === 0"
            @click="confirmExcelBatchImport"
          >
            <IconRenderer name="Check" :size="14" />
            <span>一键加密入库 ({{ excelPreviewEntries.length }} 条)</span>
          </button>
        </div>
      </div>
    </div>

    <!-- MODAL 3: Secondary Delete Confirmation Modal -->
    <div
      v-if="pendingDeleteTarget"
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-xs"
      @click.self="pendingDeleteTarget = null"
    >
      <div
        class="w-full max-w-sm rounded-2xl border p-5 shadow-2xl space-y-4"
        :class="store.isDark ? 'bg-[#111726] border-slate-700 text-white' : 'bg-white border-slate-200 text-slate-900'"
      >
        <div class="flex items-start gap-3">
          <div class="p-2 rounded-xl bg-rose-500/15 text-rose-500 shrink-0">
            <IconRenderer name="AlertTriangle" :size="18" />
          </div>
          <div>
            <h4 class="text-sm font-bold">
              {{ pendingDeleteTarget.type === 'entry' ? '确认删除整个密保资源？' : '确认移除该子账号？' }}
            </h4>
            <p class="text-xs text-slate-400 mt-1 leading-relaxed">
              <template v-if="pendingDeleteTarget.type === 'entry'">
                即将从本地加密库永久移除「<span class="font-semibold text-rose-400">{{ pendingDeleteTarget.entry.title }}</span>」及其挂载的
                {{ getEffectiveAccounts(pendingDeleteTarget.entry).length }} 组账号凭证，此操作不可撤销。
              </template>
              <template v-else>
                即将从「{{ pendingDeleteTarget.entry.title }}」中移除账号
                <code class="font-mono text-rose-400">{{
                  pendingDeleteTarget.subAccount?.username || pendingDeleteTarget.subAccount?.role
                }}</code>。
              </template>
            </p>
          </div>
        </div>

        <div class="flex items-center justify-end gap-2 pt-1">
          <button
            type="button"
            class="btn-secondary !py-1.5 !px-3 text-xs"
            @click="pendingDeleteTarget = null"
          >
            取消
          </button>
          <button
            type="button"
            class="px-3.5 py-1.5 rounded-xl text-xs font-semibold bg-rose-600 hover:bg-rose-500 text-white cursor-pointer transition-colors"
            @click="confirmDeleteTarget"
          >
            确认删除
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
