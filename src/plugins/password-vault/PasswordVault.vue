<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import { VaultEntry, encryptVaultData, decryptVaultData, kvGet, kvPut } from '@/bridge/backend'
import IconRenderer from '@/components/IconRenderer.vue'

const store = useLauncherStore()

const DEFAULT_VAULT_ITEMS: VaultEntry[] = [
  {
    id: 'v-github-prod',
    title: 'GitHub Enterprise 主账号',
    username: 'architect@omnibox.dev',
    password: 'Gh#9928!Omni$Vault_2026',
    url: 'https://github.com',
    category: '研发与云',
    notes: '包含组织 Owner 权限与 Actions 发布密钥',
    updatedAt: Date.now() - 3600000 * 12
  },
  {
    id: 'v-aws-console',
    title: 'AWS 亚太生产集群 IAM',
    username: 'liyj-admin@aws.internal',
    password: 'Aws!Prod_Kms#88412_Xp',
    url: 'https://console.aws.amazon.com',
    category: '研发与云',
    notes: 'ap-east-1 生产环境主控制台',
    updatedAt: Date.now() - 3600000 * 36
  },
  {
    id: 'v-postgres-master',
    title: '核心 RDS PostgreSQL 主库',
    username: 'pg_superuser',
    password: 'PgSql#2026$Encrypted_91',
    url: 'postgres://db.internal:5432',
    category: '内部系统',
    notes: '仅限 VPN 内网隧道访问',
    updatedAt: Date.now() - 3600000 * 72
  }
]

const isUnlocked = ref(false)
const masterPasswordInput = ref('omnibox2026')
const currentMasterKey = ref('')
const unlockError = ref('')

const vaultItems = ref<VaultEntry[]>([])
const selectedCategory = ref('全部')
const selectedId = ref<string>('v-github-prod')
const showPlainPassword = ref(false)
const isAddingNew = ref(false)

// Password Generator State
const genLength = ref(20)
const genUpper = ref(true)
const genNumbers = ref(true)
const genSymbols = ref(true)
const generatedPassword = ref('')

// New Item Form
const formTitle = ref('')
const formUsername = ref('')
const formPassword = ref('')
const formUrl = ref('')
const formCategory = ref('研发与云')
const formNotes = ref('')

const categories = ['全部', '研发与云', '内部系统', '个人账号']

const filteredItems = computed(() => {
  const q = store.subInputQuery.trim().toLowerCase()
  return vaultItems.value.filter((item) => {
    if (selectedCategory.value !== '全部' && item.category !== selectedCategory.value) {
      return false
    }
    if (!q) return true
    return (
      item.title.toLowerCase().includes(q) ||
      item.username.toLowerCase().includes(q) ||
      item.url.toLowerCase().includes(q) ||
      item.notes.toLowerCase().includes(q)
    )
  })
})

const activeItem = computed(() => {
  return (
    filteredItems.value.find((i) => i.id === selectedId.value) ||
    filteredItems.value[0] ||
    null
  )
})

function generateStrongPassword() {
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
  if (isAddingNew.value) {
    formPassword.value = out
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
    vaultItems.value = DEFAULT_VAULT_ITEMS
    currentMasterKey.value = pwd
    isUnlocked.value = true
    store.showToast('密码本已使用 Argon2id + AES-256-GCM 初始化加密并解锁')
    return
  }

  try {
    const plainJson = await decryptVaultData(pwd, encryptedPacket)
    vaultItems.value = JSON.parse(plainJson) as VaultEntry[]
    currentMasterKey.value = pwd
    isUnlocked.value = true
    if (vaultItems.value.length > 0) {
      selectedId.value = vaultItems.value[0].id
    }
    store.showToast('AES-256-GCM 密文校验通过，保险箱已解锁')
  } catch {
    unlockError.value = '主密码验证失败（默认演示密码为 omnibox2026）'
  }
}

async function persistVault() {
  if (!currentMasterKey.value) return
  const packet = await encryptVaultData(currentMasterKey.value, JSON.stringify(vaultItems.value))
  await kvPut('password-vault', 'cipher_blob', packet)
}

function lockVault() {
  isUnlocked.value = false
  currentMasterKey.value = ''
  showPlainPassword.value = false
  store.showToast('密码本已安全锁定，内存密钥已擦除', 'info')
}

async function saveNewEntry() {
  if (!formTitle.value.trim() || !formPassword.value.trim()) {
    store.showToast('请填写标题与密码', 'warning')
    return
  }
  const entry: VaultEntry = {
    id: `v-${Date.now()}`,
    title: formTitle.value.trim(),
    username: formUsername.value.trim(),
    password: formPassword.value,
    url: formUrl.value.trim(),
    category: formCategory.value,
    notes: formNotes.value.trim(),
    updatedAt: Date.now()
  }
  vaultItems.value.unshift(entry)
  selectedId.value = entry.id
  isAddingNew.value = false
  formTitle.value = ''
  formUsername.value = ''
  formPassword.value = ''
  formUrl.value = ''
  formNotes.value = ''
  await persistVault()
  store.showToast(`已加密保存：${entry.title}`)
}

async function deleteEntry(id: string) {
  vaultItems.value = vaultItems.value.filter((i) => i.id !== id)
  await persistVault()
  store.showToast('条目已从加密库移除', 'info')
}

async function copyField(text: string, label: string) {
  await navigator.clipboard.writeText(text)
  store.showToast(`已复制${label}`)
}

onMounted(() => {
  generateStrongPassword()
})
</script>

<template>
  <div
    class="flex-1 h-full min-h-[500px] select-none flex flex-col"
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
        密码保险箱已加密锁定
      </h2>
      <p class="mt-1 text-xs text-slate-400 max-w-md">
        采用 <code class="font-mono text-indigo-500">Argon2id</code> 密钥派生与 <code class="font-mono text-indigo-500">AES-256-GCM</code> 认证加密，密钥仅在解锁期间驻留内存
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
        <button
          type="submit"
          class="btn-primary w-full !py-2"
        >
          <IconRenderer name="Unlock" :size="14" />
          <span>验证主密码并解锁保险箱</span>
        </button>
      </form>
    </div>

    <!-- State 2: Unlocked Vault Workspace -->
    <div v-else class="flex-1 flex min-h-0 overflow-hidden">
      <!-- Left Column: Categories & Entries List -->
      <aside
        class="w-64 border-r flex flex-col shrink-0"
        :class="store.isDark ? 'bg-obsidian-950/40 border-slate-800/80' : 'bg-slate-100/70 border-slate-200/80'"
      >
        <div class="flex items-center justify-between px-3 py-2 border-b border-inherit">
          <div class="flex items-center gap-1 overflow-x-auto">
            <button
              v-for="cat in categories"
              :key="cat"
              type="button"
              class="px-2 py-0.5 rounded-md text-[11px] font-medium cursor-pointer transition-colors shrink-0"
              :class="
                selectedCategory === cat
                  ? store.isDark
                    ? 'bg-indigo-500/20 text-indigo-300'
                    : 'bg-indigo-50 text-indigo-600'
                  : 'text-slate-400 hover:text-slate-700 dark:hover:text-slate-200'
              "
              @click="selectedCategory = cat"
            >
              {{ cat }}
            </button>
          </div>
        </div>

        <div class="flex-1 overflow-y-auto p-2 space-y-1">
          <div
            v-for="item in filteredItems"
            :key="item.id"
            class="px-3 py-2.5 rounded-xl cursor-pointer transition-all"
            :class="
              item.id === selectedId && !isAddingNew
                ? store.isDark
                  ? 'bg-indigo-500/15 text-indigo-300 ring-1 ring-indigo-500/30'
                  : 'bg-white text-indigo-600 shadow-2xs ring-1 ring-slate-200/90'
                : store.isDark
                  ? 'text-slate-300 hover:bg-slate-800/50'
                  : 'text-slate-700 hover:bg-white/60'
            "
            @click="selectedId = item.id; isAddingNew = false"
          >
            <div class="text-xs font-semibold truncate">{{ item.title }}</div>
            <div class="text-[11px] text-slate-400 font-mono truncate mt-0.5">
              {{ item.username }}
            </div>
          </div>
        </div>

        <div class="p-2 border-t border-inherit flex items-center justify-between gap-2">
          <button
            type="button"
            class="btn-primary flex-1"
            @click="isAddingNew = true; generateStrongPassword()"
          >
            <IconRenderer name="Plus" :size="14" />
            <span>新增密保条目</span>
          </button>
          <button
            type="button"
            title="立即锁定密码本"
            aria-label="锁定密码本"
            class="btn-secondary !p-1.5"
            @click="lockVault"
          >
            <IconRenderer name="Lock" :size="14" />
          </button>
        </div>
      </aside>

      <!-- Right Column: Entry Inspector or New Entry Form + Password Generator -->
      <div
        class="flex-1 flex flex-col min-w-0 overflow-y-auto p-5"
        :class="store.isDark ? 'bg-transparent' : 'bg-[#F8FAFC]'"
      >
        <!-- New Item Creation Form -->
        <template v-if="isAddingNew">
          <div class="flex items-center justify-between mb-4">
            <h3 class="text-sm font-bold">新增 AES-256-GCM 加密账号</h3>
            <button
              type="button"
              class="text-xs text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
              @click="isAddingNew = false"
            >
              取消
            </button>
          </div>

          <div class="grid grid-cols-2 gap-3 text-xs">
            <div>
              <label class="block text-slate-400 mb-1">网站或服务名称</label>
              <input
                v-model="formTitle"
                type="text"
                placeholder="如：GitHub / 阿里云"
                class="w-full px-3 py-2 rounded-xl border outline-none focus:ring-2 focus:ring-indigo-500/25"
                :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200/90'"
              />
            </div>
            <div>
              <label class="block text-slate-400 mb-1">所属分类</label>
              <select
                v-model="formCategory"
                class="w-full px-3 py-2 rounded-xl border outline-none focus:ring-2 focus:ring-indigo-500/25"
                :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200/90'"
              >
                <option value="研发与云">研发与云</option>
                <option value="内部系统">内部系统</option>
                <option value="个人账号">个人账号</option>
              </select>
            </div>
            <div>
              <label class="block text-slate-400 mb-1">登录账号 / 邮箱</label>
              <input
                v-model="formUsername"
                type="text"
                placeholder="user@example.com"
                class="w-full px-3 py-2 rounded-xl font-mono border outline-none focus:ring-2 focus:ring-indigo-500/25"
                :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200/90'"
              />
            </div>
            <div>
              <label class="block text-slate-400 mb-1">密码原文</label>
              <input
                v-model="formPassword"
                type="text"
                class="w-full px-3 py-2 rounded-xl font-mono border outline-none text-indigo-500 focus:ring-2 focus:ring-indigo-500/25"
                :class="store.isDark ? 'bg-obsidian-950 border-slate-700' : 'bg-white border-slate-200/90'"
              />
            </div>
            <div class="col-span-2">
              <label class="block text-slate-400 mb-1">网址 URL / 连接串</label>
              <input
                v-model="formUrl"
                type="text"
                placeholder="https://..."
                class="w-full px-3 py-2 rounded-xl font-mono border outline-none focus:ring-2 focus:ring-indigo-500/25"
                :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200/90'"
              />
            </div>
            <div class="col-span-2">
              <label class="block text-slate-400 mb-1">安全备注</label>
              <input
                v-model="formNotes"
                type="text"
                placeholder="恢复码、权限说明..."
                class="w-full px-3 py-2 rounded-xl border outline-none focus:ring-2 focus:ring-indigo-500/25"
                :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200/90'"
              />
            </div>
          </div>

          <button
            type="button"
            class="btn-primary mt-4 !py-2"
            @click="saveNewEntry"
          >
            加密写入本地保险箱
          </button>
        </template>

        <!-- Existing Item Details View -->
        <template v-else-if="activeItem">
          <div class="flex items-start justify-between pb-4 border-b" :class="store.isDark ? 'border-slate-800' : 'border-slate-200/80'">
            <div>
              <div class="flex items-center gap-2">
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
              <p class="text-xs font-mono text-slate-400 mt-1">{{ activeItem.url }}</p>
            </div>
            <button
              type="button"
              aria-label="删除此密码条目"
              class="p-1.5 rounded-lg text-slate-400 hover:text-rose-500 hover:bg-rose-500/10 cursor-pointer"
              @click="deleteEntry(activeItem.id)"
            >
              <IconRenderer name="Trash2" :size="15" />
            </button>
          </div>

          <div class="mt-4 space-y-3 text-xs">
            <!-- Username Row -->
            <div
              class="flex items-center justify-between p-3 rounded-xl border"
              :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800' : 'bg-white border-slate-200/80 shadow-2xs'"
            >
              <div>
                <div class="text-[11px] text-slate-400">登录账号 / 用户名</div>
                <div class="font-mono text-sm mt-0.5 select-text">{{ activeItem.username }}</div>
              </div>
              <button
                type="button"
                class="btn-secondary"
                @click="copyField(activeItem.username, '账号')"
              >
                <IconRenderer name="Copy" :size="13" />
                <span>复制账号</span>
              </button>
            </div>

            <!-- Password Row -->
            <div
              class="flex items-center justify-between p-3 rounded-xl border"
              :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800' : 'bg-white border-slate-200/80 shadow-2xs'"
            >
              <div class="min-w-0 flex-1 pr-3">
                <div class="text-[11px] text-slate-400">加密密码 (AES-256-GCM)</div>
                <div
                  class="font-mono text-sm mt-0.5 tracking-wider truncate select-text"
                  :class="store.isDark ? 'text-indigo-300' : 'text-indigo-600'"
                >
                  {{ showPlainPassword ? activeItem.password : '••••••••••••••••••••' }}
                </div>
              </div>
              <div class="flex items-center gap-1.5 shrink-0">
                <button
                  type="button"
                  :aria-label="showPlainPassword ? '隐藏密码明文' : '显示密码明文'"
                  class="btn-secondary !p-1.5"
                  @click="showPlainPassword = !showPlainPassword"
                >
                  <IconRenderer :name="showPlainPassword ? 'EyeOff' : 'Eye'" :size="14" />
                </button>
                <button
                  type="button"
                  class="btn-primary"
                  @click="copyField(activeItem.password, '密码')"
                >
                  <IconRenderer name="Copy" :size="13" />
                  <span>复制密码</span>
                </button>
              </div>
            </div>

            <!-- Notes -->
            <div
              v-if="activeItem.notes"
              class="p-3 rounded-xl border"
              :class="store.isDark ? 'bg-obsidian-950/40 border-slate-800/60' : 'bg-white border-slate-200/80 shadow-2xs'"
            >
              <div class="text-[11px] text-slate-400">安全备注</div>
              <div class="mt-1" :class="store.isDark ? 'text-slate-300' : 'text-slate-700'">{{ activeItem.notes }}</div>
            </div>
          </div>
        </template>

        <!-- Built-in Strong Password Generator Box -->
        <div
          class="mt-auto pt-4 border-t"
          :class="store.isDark ? 'border-slate-800/80' : 'border-slate-200/80'"
        >
          <div class="flex items-center justify-between text-xs mb-2">
            <span class="font-semibold text-slate-400">内置高熵随机密码生成器 ({{ genLength }} 位)</span>
            <div class="flex items-center gap-3 text-[11px] text-slate-400">
              <label class="flex items-center gap-1 cursor-pointer">
                <input v-model="genUpper" type="checkbox" class="accent-indigo-600" @change="generateStrongPassword" />
                <span>A-Z</span>
              </label>
              <label class="flex items-center gap-1 cursor-pointer">
                <input v-model="genNumbers" type="checkbox" class="accent-indigo-600" @change="generateStrongPassword" />
                <span>0-9</span>
              </label>
              <label class="flex items-center gap-1 cursor-pointer">
                <input v-model="genSymbols" type="checkbox" class="accent-indigo-600" @change="generateStrongPassword" />
                <span>!@#$</span>
              </label>
            </div>
          </div>

          <div class="flex items-center gap-2">
            <input
              v-model.number="genLength"
              type="range"
              min="8"
              max="40"
              aria-label="随机密码长度滑块"
              class="w-24 accent-indigo-600 cursor-pointer"
              @input="generateStrongPassword"
            />
            <div
              class="flex-1 px-3 py-1.5 rounded-xl font-mono text-xs border truncate select-text"
              :class="store.isDark ? 'bg-obsidian-950 border-slate-800 text-indigo-300' : 'bg-white border-slate-200/90 text-indigo-700'"
            >
              {{ generatedPassword }}
            </div>
            <button
              type="button"
              title="重新生成随机密码"
              aria-label="重新生成随机密码"
              class="btn-secondary !p-1.5"
              @click="generateStrongPassword"
            >
              <IconRenderer name="RefreshCw" :size="14" />
            </button>
            <button
              type="button"
              class="btn-secondary"
              @click="copyField(generatedPassword, '生成的随机强密码')"
            >
              复制
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
