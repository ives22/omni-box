<script setup lang="ts">
import { ref, computed, watch, onMounted, nextTick } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import {
  requestAiChatCompletion,
  kvGet,
  kvPut,
  AiChatMessageInput
} from '@/bridge/backend'
import IconRenderer from '@/components/IconRenderer.vue'

export interface AiPromptRole {
  id: string
  name: string
  icon: string
  badge: string
  desc: string
  systemPrompt: string
  placeholder: string
  samples: string[]
  isCustom?: boolean
}

export interface AiChatTurn {
  id: string
  role: 'user' | 'assistant'
  content: string
  reasoningContent?: string | null
  model?: string
  latencyMs?: number
  totalTokens?: number | null
  createdAt: number
  isError?: boolean
}

export interface AiChatSession {
  id: string
  title: string
  roleId: string
  updatedAt: number
  turns: AiChatTurn[]
}

interface ParsedBlock {
  type: 'text' | 'code'
  lang?: string
  content: string
}

const BUILTIN_ROLES: AiPromptRole[] = [
  {
    id: 'general',
    name: '全能极客助手',
    icon: 'Sparkles',
    badge: '通用问答',
    desc: '解答技术架构、算法、日常百科与逻辑推演，回答干练精准',
    systemPrompt:
      '你是 OmniBox 桌面极客工作台的内置 AI 助手。请用清晰、准确、高信息密度的中文回答用户问题。涉及代码或命令时，请使用带语言标识的 Markdown 代码块包裹。',
    placeholder: '输入任何问题，按 Enter 发送，Shift + Enter 换行...',
    samples: [
      '用通俗的语言解释 Rust 的所有权与借用检查机制',
      '对比 HTTP/2 与 HTTP/3 (QUIC) 的核心性能差异',
      '写一个 TypeScript 并发限流器 (p-limit) 的精简实现',
      'macOS 上如何查看某个端口（如 8080）被哪个进程占用并杀掉？'
    ]
  },
  {
    id: 'translate',
    name: '沉浸式中英互译',
    icon: 'Globe',
    badge: '智能翻译',
    desc: '自动识别输入语言：中文译为地道英文，外文译为流畅中文并附专业术语解析',
    systemPrompt:
      '你是一位精通计算机科学与商务表达的资深双语翻译专家。\n1. 若用户输入中文，请将其翻译为地道、自然、专业的英文（提供【标准译文】与【极简/技术表达】两个版本）；\n2. 若用户输入英文或其他外语，请将其准确翻译为流畅自然的中文，并提炼其中的关键专业术语或短语；\n3. 若用户输入的是单个词语或短句，请额外给出代码开发常用的变量命名建议（camelCase / snake_case）。',
    placeholder: '粘贴待翻译的中文、英文段落、技术文档或单词...',
    samples: [
      '幂等性校验与分布式锁超时自动续期',
      'Zero-copy deserialization significantly reduces memory allocation overhead in high-throughput pipelines.',
      '账户余额冻结流水表',
      'Graceful degradation and circuit breaker pattern'
    ]
  },
  {
    id: 'code-explain',
    name: '代码审查与排障',
    icon: 'Code2',
    badge: '代码诊断',
    desc: '解释复杂代码逻辑、定位报错堆栈根因、发现潜在 Bug 并给出优化重构方案',
    systemPrompt:
      '你是一位首席软件工程师与代码审查专家。针对用户提供的代码片段或错误日志（Stack Trace）：\n1. 先用一句话指出核心功能或报错的直接根因；\n2. 逐步拆解关键逻辑或问题点（时间/空间复杂度、并发安全、边界条件）；\n3. 给出修复或重构优化后的完整可运行代码。',
    placeholder: '粘贴代码片段或报错堆栈日志 (Stack Trace)...',
    samples: [
      '为什么 JavaScript 中 0.1 + 0.2 !== 0.3？在金融计算中如何规避？',
      '解释 Vue 3 中 ref 与 shallowRef 的响应式触发区别及性能场景',
      '排查 SQL 查询慢的原因：SELECT * FROM orders WHERE YEAR(created_at) = 2026',
      'Rust 报错：cannot borrow `*self` as mutable more than once at a time 如何修复？'
    ]
  },
  {
    id: 'shell-sql',
    name: 'Shell / SQL / Git 生成',
    icon: 'Terminal',
    badge: '命令生成',
    desc: '用自然语言描述需求，直接生成可一键复制执行的 Shell、Git 或 SQL 语句',
    systemPrompt:
      '你是命令行与数据库专家。根据用户的自然语言描述，直接输出最精准、安全的 macOS/Linux Shell 命令、Git 指令或 SQL 查询语句。\n要求：\n1. 第一部分直接用代码块给出可直接复制执行的命令；\n2. 第二部分简明扼要地解释每个参数或子句的作用；\n3. 若命令具有破坏性（如删除文件或修改历史），请加上安全提示。',
    placeholder: '描述你想执行的命令行操作、Git 撤销或 SQL 查询需求...',
    samples: [
      '查找当前目录下所有大于 100MB 的文件并按大小降序排列',
      'Git 已经 commit 但还没 push，如何修改最后一次提交的 message 或撤回提交但保留改动？',
      '使用 ffmpeg 将 input.mp4 压缩为 720p 并提取音频为 mp3',
      '统计 Nginx 日志中访问量最高的前 10 个 IP 地址'
    ]
  },
  {
    id: 'regex',
    name: '正则表达式生成器',
    icon: 'Braces',
    badge: '正则专家',
    desc: '描述匹配规则，自动生成高兼容正则表达式、分组释义及匹配测试代码',
    systemPrompt:
      '你是正则表达式专家。请根据用户的匹配需求：\n1. 给出标准的正则表达式字面量；\n2. 逐一拆解解释正则中每个元字符和捕获分组的含义；\n3. 提供 JavaScript/TypeScript 和 Rust/Python 的校验测试代码示例及正反测试用例。',
    placeholder: '描述你需要匹配或提取的文本格式（如手机号、强密码、URL、版本号）...',
    samples: [
      '匹配中国大陆 11 位手机号（支持可选的 +86 前缀）',
      '校验强密码：至少 8 位，必须包含大小写字母、数字和特殊符号',
      '从 Markdown 文本中提取所有图片链接 ![alt](url) 的 url 部分',
      '匹配合法的 IPv4 地址及可选的 /0~/32 CIDR 前缀'
    ]
  },
  {
    id: 'naming',
    name: '变量与函数命名神器',
    icon: 'Hash',
    badge: '命名规范',
    desc: '输入中文业务概念，生成符合大厂工程规范的多风格英文标识符命名',
    systemPrompt:
      '你是代码规范与领域驱动设计（DDD）命名专家。根据用户输入的中文业务描述，提供 5 组地道、精准、简练的英文命名建议，并分别展示：\n- 小驼峰 (`camelCase`，用于函数/变量)\n- 大驼峰 (`PascalCase`，用于类/组件/类型)\n- 下划线 (`snake_case`，用于数据库字段/Rust/Python)\n- 常量大写 (`UPPER_SNAKE_CASE`)\n同时附带简短的语境区分说明。',
    placeholder: '输入中文业务含义（如：工作台子网掩码计算结果、防抖搜索触发器）...',
    samples: [
      '防抖搜索触发器与空状态占位符',
      '密码本自动锁定倒计时分钟数',
      '用户权限树形菜单懒加载节点',
      '订单支付超时自动取消定时任务'
    ]
  },
  {
    id: 'writing',
    name: '文本润色与总结提炼',
    icon: 'FileText',
    badge: '效率写作',
    desc: '提炼长文核心要点、润色技术文档与邮件、将零散记录转化为结构化周报',
    systemPrompt:
      '你是资深技术编辑与高效办公专家。请根据用户输入的内容完成总结提炼、润色改写或结构化周报整理。输出应条理分明、重点突出、语言精炼专业。',
    placeholder: '粘贴会议记录、长文章或零散工作事项，交由 AI 润色或提炼摘要...',
    samples: [
      '把这几条零散记录整理成结构化研发周报：1. 完成 IP 工作台与 CIDR 划分；2. 上线单位换算与 1024/1000 进制切换；3. 新增 macOS 右上角状态栏托盘管理与单实例保护。',
      '帮我写一封礼貌催促跨部门接口联调进度的简短沟通消息',
      '如何写出高质量的 Git Commit Message（遵循 Conventional Commits 规范）？'
    ]
  }
]

const store = useLauncherStore()

const sidebarTab = ref<'roles' | 'history'>('roles')
const customRoles = ref<AiPromptRole[]>([])
const sessions = ref<AiChatSession[]>([])
const activeSessionId = ref<string>('')
const activeRoleId = ref<string>('general')

const inputPrompt = ref('')
const isGenerating = ref(false)
const copiedKey = ref<string | null>(null)
const expandedReasoningIds = ref<Record<string, boolean>>({})
const messagesScrollRef = ref<HTMLElement | null>(null)

// Custom Role Modal State
const showAddRoleModal = ref(false)
const newRoleName = ref('')
const newRoleBadge = ref('自定义')
const newRoleDesc = ref('')
const newRoleSysPrompt = ref('')

const allRoles = computed<AiPromptRole[]>(() => [
  ...BUILTIN_ROLES,
  ...customRoles.value
])

const activeRole = computed<AiPromptRole>(() => {
  return allRoles.value.find((r) => r.id === activeRoleId.value) || BUILTIN_ROLES[0]
})

const filteredRoles = computed(() => {
  const q = store.subInputQuery.trim().toLowerCase()
  if (!q) return allRoles.value
  return allRoles.value.filter(
    (r) =>
      r.name.toLowerCase().includes(q) ||
      r.badge.toLowerCase().includes(q) ||
      r.desc.toLowerCase().includes(q)
  )
})

const filteredSessions = computed(() => {
  const q = store.subInputQuery.trim().toLowerCase()
  if (!q) return sessions.value
  return sessions.value.filter(
    (s) =>
      s.title.toLowerCase().includes(q) ||
      s.turns.some((t) => t.content.toLowerCase().includes(q))
  )
})

const activeSession = computed<AiChatSession | null>(() => {
  return sessions.value.find((s) => s.id === activeSessionId.value) || null
})

const currentProviderModels = computed<string[]>(() => {
  const prov = store.userSettings.aiProvider || 'OpenAI'
  const profile = store.userSettings.aiProviderProfiles?.[prov]
  const list = profile?.availableModels || []
  const curModel = store.userSettings.aiModelName || ''
  if (curModel && !list.includes(curModel)) {
    return [curModel, ...list]
  }
  return list.length > 0 ? list : [curModel || 'gpt-4o']
})

function markCopied(key: string) {
  copiedKey.value = key
  setTimeout(() => {
    if (copiedKey.value === key) copiedKey.value = null
  }, 1500)
}

async function copyText(text: string, label: string, key: string) {
  try {
    await navigator.clipboard.writeText(text)
    markCopied(key)
    store.showToast(`已复制 ${label}`)
  } catch {
    // ignore
  }
}

function scrollToBottom() {
  nextTick(() => {
    if (messagesScrollRef.value) {
      messagesScrollRef.value.scrollTop = messagesScrollRef.value.scrollHeight
    }
  })
}

async function persistSessions() {
  await kvPut('ai-studio', 'sessions', sessions.value.slice(0, 50))
}

async function persistCustomRoles() {
  await kvPut('ai-studio', 'custom-roles', customRoles.value)
}

function createNewSession(roleId = activeRoleId.value): AiChatSession {
  const role = allRoles.value.find((r) => r.id === roleId) || BUILTIN_ROLES[0]
  const s: AiChatSession = {
    id: `sess-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    title: `${role.name} · 新对话`,
    roleId: role.id,
    updatedAt: Date.now(),
    turns: []
  }
  sessions.value.unshift(s)
  activeSessionId.value = s.id
  activeRoleId.value = role.id
  persistSessions()
  return s
}

function selectRole(roleId: string) {
  activeRoleId.value = roleId
  const cur = activeSession.value
  if (!cur || cur.turns.length > 0) {
    createNewSession(roleId)
  } else {
    cur.roleId = roleId
    const role = allRoles.value.find((r) => r.id === roleId) || BUILTIN_ROLES[0]
    cur.title = `${role.name} · 新对话`
    persistSessions()
  }
}

function selectSession(session: AiChatSession) {
  activeSessionId.value = session.id
  if (allRoles.value.some((r) => r.id === session.roleId)) {
    activeRoleId.value = session.roleId
  }
  scrollToBottom()
}

function deleteSession(sessionId: string) {
  sessions.value = sessions.value.filter((s) => s.id !== sessionId)
  if (activeSessionId.value === sessionId) {
    if (sessions.value.length > 0) {
      activeSessionId.value = sessions.value[0].id
      activeRoleId.value = sessions.value[0].roleId
    } else {
      createNewSession(activeRoleId.value)
    }
  }
  persistSessions()
}

function clearCurrentSessionTurns() {
  if (!activeSession.value) return
  activeSession.value.turns = []
  activeSession.value.title = `${activeRole.value.name} · 新对话`
  activeSession.value.updatedAt = Date.now()
  persistSessions()
}

async function saveNewCustomRole() {
  if (!newRoleName.value.trim() || !newRoleSysPrompt.value.trim()) {
    store.showToast('请填写角色名称与系统提示词', 'warning')
    return
  }
  const role: AiPromptRole = {
    id: `custom-${Date.now()}`,
    name: newRoleName.value.trim(),
    icon: 'Sparkles',
    badge: newRoleBadge.value.trim() || '自定义',
    desc: newRoleDesc.value.trim() || '自定义 AI 角色指令',
    systemPrompt: newRoleSysPrompt.value.trim(),
    placeholder: `向「${newRoleName.value.trim()}」提问...`,
    samples: [],
    isCustom: true
  }
  customRoles.value.push(role)
  await persistCustomRoles()
  showAddRoleModal.value = false
  newRoleName.value = ''
  newRoleDesc.value = ''
  newRoleSysPrompt.value = ''
  selectRole(role.id)
  store.showToast(`已创建自定义角色「${role.name}」`)
}

async function deleteCustomRole(roleId: string) {
  customRoles.value = customRoles.value.filter((r) => r.id !== roleId)
  await persistCustomRoles()
  if (activeRoleId.value === roleId) {
    selectRole('general')
  }
}

async function pasteClipboardAndAsk() {
  try {
    const clip = await navigator.clipboard.readText()
    if (!clip || !clip.trim()) {
      store.showToast('剪贴板暂无文本内容', 'warning')
      return
    }
    inputPrompt.value = clip.trim()
    await sendMessage()
  } catch {
    store.showToast('无法读取剪贴板', 'warning')
  }
}

async function sendMessage(overrideText?: string) {
  const text = (overrideText !== undefined ? overrideText : inputPrompt.value).trim()
  if (!text || isGenerating.value) return

  if (overrideText === undefined) {
    inputPrompt.value = ''
  }

  let session = activeSession.value
  if (!session) {
    session = createNewSession(activeRoleId.value)
  }

  // Update session title on first user message
  if (session.turns.length === 0) {
    const shortTitle = text.replace(/\s+/g, ' ').slice(0, 22)
    session.title = shortTitle.length < text.length ? `${shortTitle}...` : shortTitle
  }

  const userTurn: AiChatTurn = {
    id: `turn-u-${Date.now()}`,
    role: 'user',
    content: text,
    createdAt: Date.now()
  }
  session.turns.push(userTurn)
  session.updatedAt = Date.now()
  scrollToBottom()

  const provider = store.userSettings.aiProvider || 'OpenAI'
  const apiKey = (store.userSettings.aiApiKey || '').trim()
  const baseUrl = (store.userSettings.aiBaseUrl || '').trim()
  const model = (store.userSettings.aiModelName || 'gpt-4o').trim()

  if (!apiKey && provider !== 'Ollama 本地模型') {
    session.turns.push({
      id: `turn-a-${Date.now()}`,
      role: 'assistant',
      content: `尚未配置 **${provider}** 的 API Key。\n请点击右上角「配置模型」按钮（或前往「个人中心 -> AI 模型」）填写您的 API Endpoint 与密钥。`,
      model,
      createdAt: Date.now(),
      isError: true
    })
    await persistSessions()
    scrollToBottom()
    return
  }

  isGenerating.value = true

  // Build context messages (system prompt + last 10 turns for multi-turn continuity)
  const historyMessages: AiChatMessageInput[] = [
    {
      role: 'system',
      content: activeRole.value.systemPrompt
    }
  ]

  const recentTurns = session.turns.filter((t) => !t.isError).slice(-10)
  for (const t of recentTurns) {
    historyMessages.push({
      role: t.role,
      content: t.content
    })
  }

  const proxyUrl =
    store.userSettings.proxyEnabled && store.userSettings.proxyHost
      ? store.userSettings.proxyHost.trim()
      : undefined

  try {
    const res = await requestAiChatCompletion({
      provider,
      baseUrl,
      apiKey,
      model,
      messages: historyMessages,
      temperature: store.userSettings.aiTemperature ?? 0.7,
      proxyUrl
    })

    session.turns.push({
      id: `turn-a-${Date.now()}`,
      role: 'assistant',
      content: res.content,
      reasoningContent: res.reasoningContent || null,
      model: res.model || model,
      latencyMs: res.latencyMs,
      totalTokens: res.totalTokens || null,
      createdAt: Date.now(),
      isError: false
    })
  } catch (e) {
    session.turns.push({
      id: `turn-a-${Date.now()}`,
      role: 'assistant',
      content: `请求失败：${String(e)}`,
      model,
      createdAt: Date.now(),
      isError: true
    })
  } finally {
    isGenerating.value = false
    session.updatedAt = Date.now()
    await persistSessions()
    scrollToBottom()
  }
}

async function regenerateLastResponse() {
  const session = activeSession.value
  if (!session || isGenerating.value || session.turns.length === 0) return
  // Find last user message
  let lastUserIdx = -1
  for (let i = session.turns.length - 1; i >= 0; i--) {
    if (session.turns[i].role === 'user') {
      lastUserIdx = i
      break
    }
  }
  if (lastUserIdx === -1) return
  const userText = session.turns[lastUserIdx].content
  session.turns = session.turns.slice(0, lastUserIdx)
  await sendMessage(userText)
}

function handleInputKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
    e.preventDefault()
    sendMessage()
  }
}

/**
 * Parse Markdown message content into structured text and fenced code blocks (` ```lang ... ``` `)
 */
function parseMessageBlocks(raw: string): ParsedBlock[] {
  if (!raw) return []
  const blocks: ParsedBlock[] = []
  const regex = /```([a-zA-Z0-9_-]*)\n?([\s\S]*?)```/g
  let lastIdx = 0
  let match: RegExpExecArray | null

  while ((match = regex.exec(raw)) !== null) {
    if (match.index > lastIdx) {
      const textPart = raw.slice(lastIdx, match.index).trim()
      if (textPart) {
        blocks.push({ type: 'text', content: textPart })
      }
    }
    blocks.push({
      type: 'code',
      lang: (match[1] || 'code').toLowerCase(),
      content: (match[2] || '').replace(/\n$/, '')
    })
    lastIdx = regex.lastIndex
  }

  if (lastIdx < raw.length) {
    const rest = raw.slice(lastIdx).trim()
    if (rest) {
      blocks.push({ type: 'text', content: rest })
    }
  }

  return blocks.length > 0 ? blocks : [{ type: 'text', content: raw }]
}

function sendCodeToPlugin(code: string, targetPlugin: 'json-studio' | 'diff-viewer' | 'calc-pad') {
  store.pluginInitialPayload = code
  store.openPlugin(targetPlugin)
}

function openAiSettingsPage() {
  ;(window as unknown as { __omniboxUserCenterInitialTab?: string }).__omniboxUserCenterInitialTab =
    'ai-models'
  store.openPlugin('user-center')
}

function consumeIncomingPayload() {
  const raw = (store.pluginInitialPayload || '').trim()
  if (!raw) return
  store.pluginInitialPayload = ''

  try {
    const parsed = JSON.parse(raw)
    if (parsed?.type === 'handoff-qa' && parsed.question) {
      const s = createNewSession('general')
      s.title = parsed.question.slice(0, 22)
      s.turns.push({
        id: `turn-u-${Date.now()}-1`,
        role: 'user',
        content: parsed.question,
        createdAt: Date.now() - 1000
      })
      if (parsed.answer) {
        s.turns.push({
          id: `turn-a-${Date.now()}-2`,
          role: 'assistant',
          content: parsed.answer,
          reasoningContent: parsed.reasoningContent || null,
          model: parsed.model || store.userSettings.aiModelName,
          createdAt: Date.now()
        })
      }
      persistSessions()
      scrollToBottom()
      return
    }

    if (parsed?.type === 'quick-action') {
      const modeMap: Record<string, string> = {
        translate: 'translate',
        explain: 'code-explain',
        summarize: 'writing',
        shell: 'shell-sql',
        chat: 'general'
      }
      const targetRoleId = modeMap[parsed.actionMode] || 'general'
      createNewSession(targetRoleId)
      if (parsed.inputText) {
        sendMessage(parsed.inputText)
      }
      return
    }
  } catch {
    // Plain text payload
    const s = createNewSession('general')
    sendMessage(raw)
    return
  }
}

watch(
  () => store.pluginInitialPayload,
  (val) => {
    if (val) consumeIncomingPayload()
  }
)

onMounted(async () => {
  customRoles.value = await kvGet<AiPromptRole[]>('ai-studio', 'custom-roles', [])
  const savedSessions = await kvGet<AiChatSession[]>('ai-studio', 'sessions', [])
  if (savedSessions && savedSessions.length > 0) {
    sessions.value = savedSessions
    activeSessionId.value = savedSessions[0].id
    activeRoleId.value = savedSessions[0].roleId || 'general'
  } else {
    createNewSession('general')
  }

  if (store.pluginInitialPayload) {
    consumeIncomingPayload()
  }
  scrollToBottom()
})
</script>

<template>
  <div
    class="flex flex-1 h-full min-h-[480px] overflow-hidden select-none"
    :class="store.isDark ? 'bg-[#0B0F19] text-slate-100' : 'bg-[#F8FAFC] text-slate-800'"
  >
    <!-- ========================================================================= -->
    <!-- Left Sidebar: Prompt Roles & Multi-Turn Session History (`210px`)         -->
    <!-- ========================================================================= -->
    <aside
      class="w-[215px] border-r flex flex-col justify-between shrink-0"
      :class="
        store.isDark
          ? 'bg-[#080C14] border-slate-800/80'
          : 'bg-[#F1F5F9]/85 border-slate-200/85'
      "
    >
      <!-- Top Segmented Switcher (Roles vs History) -->
      <div class="p-2.5 border-b" :class="store.isDark ? 'border-slate-800/80' : 'border-slate-200/80'">
        <div
          class="grid grid-cols-2 p-0.5 rounded-xl border text-xs font-semibold"
          :class="
            store.isDark
              ? 'bg-slate-950 border-slate-800'
              : 'bg-slate-200/70 border-slate-200'
          "
        >
          <button
            type="button"
            class="py-1.5 rounded-lg cursor-pointer transition-all"
            :class="
              sidebarTab === 'roles'
                ? store.isDark
                  ? 'bg-indigo-500/25 text-indigo-300'
                  : 'bg-white text-indigo-600 shadow-2xs'
                : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
            "
            @click="sidebarTab = 'roles'"
          >
            角色指令 ({{ allRoles.length }})
          </button>
          <button
            type="button"
            class="py-1.5 rounded-lg cursor-pointer transition-all"
            :class="
              sidebarTab === 'history'
                ? store.isDark
                  ? 'bg-indigo-500/25 text-indigo-300'
                  : 'bg-white text-indigo-600 shadow-2xs'
                : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
            "
            @click="sidebarTab = 'history'"
          >
            对话历史 ({{ sessions.length }})
          </button>
        </div>
      </div>

      <!-- List Content -->
      <div class="flex-1 overflow-y-auto p-2 space-y-1">
        <!-- Tab 1: Built-in & Custom AI Prompt Roles -->
        <template v-if="sidebarTab === 'roles'">
          <div
            v-for="role in filteredRoles"
            :key="role.id"
            class="group p-2.5 rounded-xl border cursor-pointer transition-all"
            :class="
              activeRoleId === role.id
                ? store.isDark
                  ? 'bg-indigo-500/15 border-indigo-500/50 text-indigo-200'
                  : 'bg-white border-indigo-300 text-indigo-900 shadow-xs'
                : store.isDark
                  ? 'bg-transparent border-transparent hover:bg-slate-900/70 text-slate-300'
                  : 'bg-transparent border-transparent hover:bg-white/70 text-slate-700'
            "
            @click="selectRole(role.id)"
          >
            <div class="flex items-center justify-between gap-1.5">
              <div class="flex items-center gap-2 min-w-0">
                <IconRenderer
                  :name="role.icon"
                  :size="14"
                  :class-name="activeRoleId === role.id ? 'text-indigo-400 shrink-0' : 'text-slate-400 shrink-0'"
                />
                <span class="text-xs font-bold truncate">{{ role.name }}</span>
              </div>
              <div class="flex items-center gap-1 shrink-0">
                <span
                  class="px-1.5 py-0.5 rounded text-[10px] font-medium"
                  :class="
                    activeRoleId === role.id
                      ? 'bg-indigo-500/20 text-indigo-300'
                      : store.isDark
                        ? 'bg-slate-800 text-slate-400'
                        : 'bg-slate-200/80 text-slate-600'
                  "
                >
                  {{ role.badge }}
                </span>
                <button
                  v-if="role.isCustom"
                  type="button"
                  title="删除自定义角色"
                  class="opacity-0 group-hover:opacity-100 text-slate-400 hover:text-rose-400 p-0.5 cursor-pointer"
                  @click.stop="deleteCustomRole(role.id)"
                >
                  <IconRenderer name="Trash2" :size="11" />
                </button>
              </div>
            </div>
            <div class="text-[10.5px] text-slate-400 mt-1 line-clamp-1">
              {{ role.desc }}
            </div>
          </div>
        </template>

        <!-- Tab 2: Conversation History -->
        <template v-else>
          <div
            v-for="sess in filteredSessions"
            :key="sess.id"
            class="group p-2.5 rounded-xl border cursor-pointer transition-all flex items-center justify-between gap-2"
            :class="
              activeSessionId === sess.id
                ? store.isDark
                  ? 'bg-indigo-500/15 border-indigo-500/50 text-indigo-200'
                  : 'bg-white border-indigo-300 text-indigo-900 shadow-xs'
                : store.isDark
                  ? 'bg-transparent border-transparent hover:bg-slate-900/70 text-slate-300'
                  : 'bg-transparent border-transparent hover:bg-white/70 text-slate-700'
            "
            @click="selectSession(sess)"
          >
            <div class="min-w-0 flex-1">
              <div class="text-xs font-semibold truncate">{{ sess.title }}</div>
              <div class="text-[10px] text-slate-400 mt-0.5 flex items-center gap-1.5">
                <span>{{ sess.turns.length }} 条消息</span>
                <span>·</span>
                <span>{{ new Date(sess.updatedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) }}</span>
              </div>
            </div>
            <button
              type="button"
              title="删除此会话"
              class="opacity-0 group-hover:opacity-100 text-slate-400 hover:text-rose-400 p-1 rounded cursor-pointer shrink-0"
              @click.stop="deleteSession(sess.id)"
            >
              <IconRenderer name="Trash2" :size="12" />
            </button>
          </div>
        </template>
      </div>

      <!-- Sidebar Bottom Actions -->
      <div
        class="p-2.5 border-t space-y-1.5"
        :class="store.isDark ? 'border-slate-800/80' : 'border-slate-200/80'"
      >
        <button
          type="button"
          class="w-full py-1.5 px-3 rounded-xl border text-xs font-semibold flex items-center justify-center gap-1.5 cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-indigo-500/20 border-indigo-500/40 text-indigo-300 hover:bg-indigo-500/30'
              : 'bg-indigo-600 border-indigo-600 text-white hover:bg-indigo-700'
          "
          @click="createNewSession(activeRoleId)"
        >
          <IconRenderer name="Plus" :size="13" />
          <span>新建对话会话</span>
        </button>

        <button
          type="button"
          class="w-full py-1.5 px-3 rounded-xl border text-[11px] font-medium flex items-center justify-center gap-1.5 cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-slate-900 border-slate-800 text-slate-300 hover:border-indigo-500/40'
              : 'bg-white border-slate-200/90 text-slate-700 hover:border-indigo-300'
          "
          @click="showAddRoleModal = true"
        >
          <IconRenderer name="Sparkles" :size="12" class-name="text-indigo-400" />
          <span>+ 新增自定义 Prompt 角色</span>
        </button>
      </div>
    </aside>

    <!-- ========================================================================= -->
    <!-- Right Main Area: Header Bar + Chat Canvas + Prompt Composer               -->
    <!-- ========================================================================= -->
    <div class="flex-1 flex flex-col min-w-0 h-full overflow-hidden">
      <!-- Top Model & Role Control Bar -->
      <div
        class="px-4 py-2.5 border-b flex items-center justify-between gap-2 shrink-0"
        :class="
          store.isDark
            ? 'bg-[#0E1422]/90 border-slate-800/80'
            : 'bg-white border-slate-200/85'
        "
      >
        <div class="flex items-center gap-2 min-w-0">
          <span
            class="inline-flex items-center justify-center w-6 h-6 rounded-lg bg-indigo-500/15 text-indigo-400 shrink-0"
          >
            <IconRenderer :name="activeRole.icon" :size="14" />
          </span>
          <div class="min-w-0">
            <div class="flex items-center gap-1.5">
              <span class="text-xs font-bold truncate">{{ activeRole.name }}</span>
              <span
                class="px-1.5 py-0.5 rounded text-[10px] font-medium shrink-0"
                :class="
                  store.isDark
                    ? 'bg-indigo-500/15 text-indigo-300'
                    : 'bg-indigo-50 text-indigo-600'
                "
              >
                {{ activeRole.badge }}
              </span>
            </div>
          </div>
        </div>

        <!-- Quick Provider & Model Selector + Settings Link -->
        <div class="flex items-center gap-1.5 shrink-0">
          <!-- Provider Switcher -->
          <select
            :value="store.userSettings.aiProvider || 'OpenAI'"
            aria-label="切换 AI 服务商"
            class="h-[28px] px-2 rounded-lg border text-[11px] font-semibold outline-none cursor-pointer"
            :class="
              store.isDark
                ? 'bg-slate-900 border-slate-700 text-slate-200'
                : 'bg-slate-50 border-slate-200 text-slate-700'
            "
            @change="(e) => store.switchAiProvider((e.target as HTMLSelectElement).value)"
          >
            <option value="OpenAI">OpenAI / 兼容中转</option>
            <option value="DeepSeek">DeepSeek</option>
            <option value="Google Gemini">Google Gemini</option>
            <option value="Anthropic Claude">Anthropic Claude</option>
            <option value="Ollama 本地模型">Ollama 本地</option>
          </select>

          <!-- Model Switcher / Input -->
          <select
            v-if="currentProviderModels.length > 1"
            :value="store.userSettings.aiModelName"
            aria-label="切换 AI 模型"
            class="h-[28px] max-w-[165px] px-2 rounded-lg border font-mono text-[11px] font-semibold outline-none cursor-pointer truncate"
            :class="
              store.isDark
                ? 'bg-slate-900 border-slate-700 text-indigo-300'
                : 'bg-indigo-50/60 border-indigo-200 text-indigo-700'
            "
            @change="
              (e) =>
                store.saveUserSettings({ aiModelName: (e.target as HTMLSelectElement).value })
            "
          >
            <option v-for="m in currentProviderModels" :key="m" :value="m">
              {{ m }}
            </option>
          </select>
          <span
            v-else
            class="px-2 py-1 rounded-lg border font-mono text-[11px] font-semibold max-w-[150px] truncate"
            :class="
              store.isDark
                ? 'bg-slate-900 border-slate-800 text-indigo-300'
                : 'bg-indigo-50/60 border-indigo-200 text-indigo-700'
            "
          >
            {{ store.userSettings.aiModelName || 'gpt-4o' }}
          </span>

          <button
            v-if="activeSession && activeSession.turns.length > 0"
            type="button"
            title="清空当前对话上下文"
            class="h-[28px] px-2 rounded-lg border text-[11px] font-medium cursor-pointer transition-colors"
            :class="
              store.isDark
                ? 'bg-slate-900 border-slate-700 text-slate-400 hover:text-slate-200'
                : 'bg-white border-slate-200 text-slate-500 hover:text-slate-800'
            "
            @click="clearCurrentSessionTurns"
          >
            清空
          </button>

          <button
            type="button"
            title="前往 AI 模型与密钥设置"
            class="h-[28px] px-2.5 rounded-lg border text-[11px] font-semibold flex items-center gap-1 cursor-pointer transition-colors"
            :class="
              store.isDark
                ? 'bg-slate-900 border-slate-700 text-slate-300 hover:border-indigo-500'
                : 'bg-white border-slate-200 text-slate-700 hover:border-indigo-400'
            "
            @click="openAiSettingsPage"
          >
            <IconRenderer name="Settings" :size="12" />
            <span>配置模型</span>
          </button>
        </div>
      </div>

      <!-- Conversation Messages Area -->
      <div
        ref="messagesScrollRef"
        class="flex-1 overflow-y-auto px-4 py-3.5 space-y-3.5 select-text"
      >
        <!-- Empty State: Role Description & Quick Sample Prompts -->
        <div
          v-if="!activeSession || activeSession.turns.length === 0"
          class="h-full flex flex-col justify-center items-center text-center px-4 py-6 space-y-4 select-none"
        >
          <div class="space-y-1.5 max-w-md">
            <div class="inline-flex items-center gap-2 px-3 py-1 rounded-full border text-xs font-bold"
              :class="
                store.isDark
                  ? 'bg-indigo-500/10 border-indigo-500/30 text-indigo-300'
                  : 'bg-indigo-50 border-indigo-200 text-indigo-700'
              "
            >
              <IconRenderer :name="activeRole.icon" :size="13" />
              <span>{{ activeRole.name }}</span>
            </div>
            <p class="text-xs text-slate-400 leading-relaxed">
              {{ activeRole.desc }}
            </p>
          </div>

          <div
            v-if="activeRole.samples.length > 0"
            class="w-full max-w-xl grid grid-cols-2 gap-2 text-left"
          >
            <button
              v-for="(sample, sIdx) in activeRole.samples"
              :key="sIdx"
              type="button"
              class="p-2.5 rounded-xl border text-xs leading-snug cursor-pointer transition-all flex items-start gap-2"
              :class="
                store.isDark
                  ? 'bg-[#121929]/85 border-slate-800/90 hover:border-indigo-500/50 text-slate-300'
                  : 'bg-white border-slate-200/85 hover:border-indigo-300 text-slate-700 shadow-2xs'
              "
              @click="sendMessage(sample)"
            >
              <IconRenderer name="CornerDownRight" :size="13" class-name="text-indigo-400 shrink-0 mt-0.5" />
              <span class="line-clamp-2">{{ sample }}</span>
            </button>
          </div>

          <button
            type="button"
            class="px-3.5 py-1.5 rounded-xl border text-xs font-semibold flex items-center gap-1.5 cursor-pointer transition-colors"
            :class="
              store.isDark
                ? 'bg-slate-900 border-slate-700 text-indigo-300 hover:border-indigo-500'
                : 'bg-white border-slate-300 text-indigo-600 hover:border-indigo-400 shadow-2xs'
            "
            @click="pasteClipboardAndAsk"
          >
            <IconRenderer name="Clipboard" :size="13" />
            <span>直接读取剪贴板内容并执行「{{ activeRole.name }}」</span>
          </button>
        </div>

        <!-- Active Conversation Turns -->
        <template v-else>
          <div
            v-for="turn in activeSession.turns"
            :key="turn.id"
            class="space-y-1.5"
          >
            <!-- User Message Bubble -->
            <div v-if="turn.role === 'user'" class="flex justify-end">
              <div
                class="max-w-[85%] px-3.5 py-2.5 rounded-2xl rounded-tr-sm text-xs leading-relaxed whitespace-pre-wrap break-words"
                :class="
                  store.isDark
                    ? 'bg-indigo-600/90 text-white'
                    : 'bg-indigo-600 text-white shadow-xs'
                "
              >
                {{ turn.content }}
              </div>
            </div>

            <!-- Assistant Message Card -->
            <div
              v-else
              class="p-3.5 rounded-2xl border space-y-2.5"
              :class="
                turn.isError
                  ? store.isDark
                    ? 'bg-rose-950/30 border-rose-500/40 text-rose-200'
                    : 'bg-rose-50 border-rose-200 text-rose-700'
                  : store.isDark
                    ? 'bg-[#121929]/90 border-slate-800/90 text-slate-100'
                    : 'bg-white border-slate-200/85 text-slate-800 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
              "
            >
              <!-- Optional Deep Reasoning / Thinking Process Accordion -->
              <div
                v-if="turn.reasoningContent"
                class="rounded-xl border overflow-hidden text-[11px]"
                :class="
                  store.isDark
                    ? 'bg-slate-950/70 border-slate-800 text-slate-400'
                    : 'bg-slate-50 border-slate-200/80 text-slate-600'
                "
              >
                <button
                  type="button"
                  class="w-full px-3 py-1.5 flex items-center justify-between font-semibold cursor-pointer"
                  @click="expandedReasoningIds[turn.id] = !expandedReasoningIds[turn.id]"
                >
                  <span class="flex items-center gap-1.5">
                    <IconRenderer name="Sparkles" :size="12" class-name="text-indigo-400" />
                    <span>深度思考过程 (Reasoning)</span>
                  </span>
                  <span>{{ expandedReasoningIds[turn.id] ? '收起 ▲' : '展开 ▼' }}</span>
                </button>
                <div
                  v-if="expandedReasoningIds[turn.id]"
                  class="px-3 py-2 border-t whitespace-pre-wrap font-mono leading-relaxed"
                  :class="store.isDark ? 'border-slate-800' : 'border-slate-200'"
                >
                  {{ turn.reasoningContent }}
                </div>
              </div>

              <!-- Structured Content Blocks (Text + Fenced Code Blocks) -->
              <div class="space-y-2.5 text-xs leading-relaxed">
                <template
                  v-for="(block, bIdx) in parseMessageBlocks(turn.content)"
                  :key="bIdx"
                >
                  <div
                    v-if="block.type === 'text'"
                    class="whitespace-pre-wrap break-words"
                  >
                    {{ block.content }}
                  </div>

                  <div
                    v-else
                    class="rounded-xl border overflow-hidden"
                    :class="
                      store.isDark
                        ? 'bg-[#090D16] border-slate-800'
                        : 'bg-slate-900 border-slate-800 text-slate-100'
                    "
                  >
                    <div
                      class="px-3 py-1.5 bg-slate-950/80 border-b border-slate-800 flex items-center justify-between text-[11px] text-slate-400 select-none"
                    >
                      <span class="font-mono font-bold uppercase text-indigo-400">
                        {{ block.lang || 'CODE' }}
                      </span>
                      <div class="flex items-center gap-2">
                        <button
                          v-if="block.lang === 'json'"
                          type="button"
                          class="hover:text-indigo-300 cursor-pointer"
                          @click="sendCodeToPlugin(block.content, 'json-studio')"
                        >
                          送入 JSON 助手
                        </button>
                        <button
                          type="button"
                          class="flex items-center gap-1 text-indigo-400 hover:text-indigo-300 font-semibold cursor-pointer"
                          @click="
                            copyText(
                              block.content,
                              `${block.lang || '代码'}片段`,
                              `code-${turn.id}-${bIdx}`
                            )
                          "
                        >
                          <IconRenderer
                            :name="copiedKey === `code-${turn.id}-${bIdx}` ? 'Check' : 'Copy'"
                            :size="11"
                          />
                          <span>
                            {{ copiedKey === `code-${turn.id}-${bIdx}` ? '已复制' : '复制代码' }}
                          </span>
                        </button>
                      </div>
                    </div>
                    <pre
                      class="p-3 font-mono text-[11.5px] leading-relaxed overflow-x-auto text-slate-100"
                    ><code>{{ block.content }}</code></pre>
                  </div>
                </template>
              </div>

              <!-- Turn Footer Meta & Actions -->
              <div
                class="pt-2 border-t flex items-center justify-between text-[10.5px] text-slate-400 select-none"
                :class="store.isDark ? 'border-slate-800/80' : 'border-slate-100'"
              >
                <div class="flex items-center gap-2 font-mono">
                  <span v-if="turn.model">{{ turn.model }}</span>
                  <span v-if="turn.latencyMs">· {{ (turn.latencyMs / 1000).toFixed(2) }}s</span>
                  <span v-if="turn.totalTokens">· {{ turn.totalTokens }} tokens</span>
                </div>
                <div class="flex items-center gap-2.5">
                  <button
                    type="button"
                    class="hover:text-indigo-400 flex items-center gap-1 cursor-pointer"
                    @click="copyText(turn.content, '完整回答', `turn-${turn.id}`)"
                  >
                    <IconRenderer
                      :name="copiedKey === `turn-${turn.id}` ? 'Check' : 'Copy'"
                      :size="11"
                    />
                    <span>{{ copiedKey === `turn-${turn.id}` ? '已复制' : '复制全文' }}</span>
                  </button>
                  <button
                    type="button"
                    class="hover:text-indigo-400 flex items-center gap-1 cursor-pointer"
                    @click="regenerateLastResponse"
                  >
                    <IconRenderer name="RefreshCw" :size="11" />
                    <span>重新生成</span>
                  </button>
                </div>
              </div>
            </div>
          </div>

          <!-- Generating Loading Indicator -->
          <div
            v-if="isGenerating"
            class="p-3.5 rounded-2xl border flex items-center gap-2.5 text-xs text-slate-400"
            :class="
              store.isDark
                ? 'bg-[#121929]/80 border-slate-800'
                : 'bg-white border-slate-200'
            "
          >
            <IconRenderer name="RefreshCw" :size="14" class-name="animate-spin text-indigo-400" />
            <span>
              正在请求
              <strong class="text-indigo-400 font-mono">{{ store.userSettings.aiModelName }}</strong>
              生成回答中...
            </span>
          </div>
        </template>
      </div>

      <!-- Bottom Input Composer -->
      <div
        class="p-3 border-t shrink-0"
        :class="
          store.isDark
            ? 'bg-[#0E1422]/95 border-slate-800/80'
            : 'bg-white border-slate-200/85'
        "
      >
        <div class="flex items-end gap-2">
          <div class="flex-1 relative">
            <textarea
              v-model="inputPrompt"
              rows="2"
              :placeholder="activeRole.placeholder"
              class="w-full p-2.5 pr-20 rounded-xl border text-xs leading-relaxed outline-none resize-none font-sans"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-700/90 text-slate-100 focus:border-indigo-500'
                  : 'bg-slate-50 border-slate-300/90 text-slate-900 focus:border-indigo-500'
              "
              @keydown="handleInputKeydown"
            ></textarea>
            <button
              type="button"
              title="读取剪贴板并直接提问"
              class="absolute right-2 bottom-2 px-2 py-1 rounded-lg border text-[10.5px] font-medium cursor-pointer transition-colors"
              :class="
                store.isDark
                  ? 'bg-slate-900 border-slate-700 text-slate-300 hover:border-indigo-500'
                  : 'bg-white border-slate-200 text-slate-600 hover:border-indigo-400'
              "
              @click="pasteClipboardAndAsk"
            >
              粘贴提问
            </button>
          </div>

          <button
            type="button"
            :disabled="isGenerating || !inputPrompt.trim()"
            class="h-[54px] px-4 rounded-xl font-semibold text-xs flex items-center gap-1.5 cursor-pointer transition-all shrink-0 disabled:opacity-40"
            :class="
              store.isDark
                ? 'bg-indigo-600 hover:bg-indigo-500 text-white'
                : 'bg-indigo-600 hover:bg-indigo-700 text-white shadow-xs'
            "
            @click="sendMessage()"
          >
            <IconRenderer name="Send" :size="14" />
            <span>发送</span>
          </button>
        </div>
      </div>
    </div>

    <!-- Modal: Create Custom AI Prompt Role -->
    <div
      v-if="showAddRoleModal"
      class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4"
      @click.self="showAddRoleModal = false"
    >
      <div
        class="w-full max-w-md p-5 rounded-2xl border space-y-3.5 text-xs"
        :class="
          store.isDark
            ? 'bg-[#111827] border-slate-700 text-slate-100'
            : 'bg-white border-slate-200 text-slate-800 shadow-2xl'
        "
      >
        <div class="flex items-center justify-between">
          <span class="text-sm font-bold">新增自定义 AI 角色指令</span>
          <button
            type="button"
            class="text-slate-400 hover:text-slate-200 cursor-pointer"
            @click="showAddRoleModal = false"
          >
            ✕
          </button>
        </div>

        <div class="space-y-2.5">
          <div>
            <label class="block text-[11px] text-slate-400 mb-1">角色名称</label>
            <input
              v-model="newRoleName"
              type="text"
              placeholder="例如：Go 架构师 / SQL 调优专家 / 小红书文案"
              class="w-full h-8 px-2.5 rounded-lg border outline-none"
              :class="
                store.isDark
                  ? 'bg-slate-900 border-slate-700 text-white'
                  : 'bg-slate-50 border-slate-300 text-slate-900'
              "
            />
          </div>

          <div class="grid grid-cols-2 gap-2">
            <div>
              <label class="block text-[11px] text-slate-400 mb-1">短标签 (Badge)</label>
              <input
                v-model="newRoleBadge"
                type="text"
                placeholder="自定义"
                class="w-full h-8 px-2.5 rounded-lg border outline-none"
                :class="
                  store.isDark
                    ? 'bg-slate-900 border-slate-700 text-white'
                    : 'bg-slate-50 border-slate-300 text-slate-900'
                "
              />
            </div>
            <div>
              <label class="block text-[11px] text-slate-400 mb-1">一句话简介</label>
              <input
                v-model="newRoleDesc"
                type="text"
                placeholder="该角色的用途说明..."
                class="w-full h-8 px-2.5 rounded-lg border outline-none"
                :class="
                  store.isDark
                    ? 'bg-slate-900 border-slate-700 text-white'
                    : 'bg-slate-50 border-slate-300 text-slate-900'
                "
              />
            </div>
          </div>

          <div>
            <label class="block text-[11px] text-slate-400 mb-1">System Prompt (系统提示词)</label>
            <textarea
              v-model="newRoleSysPrompt"
              rows="4"
              placeholder="定义 AI 的身份、输出规范与专业领域要求..."
              class="w-full p-2.5 rounded-lg border outline-none resize-none"
              :class="
                store.isDark
                  ? 'bg-slate-900 border-slate-700 text-white'
                  : 'bg-slate-50 border-slate-300 text-slate-900'
              "
            ></textarea>
          </div>
        </div>

        <div class="flex justify-end gap-2 pt-1">
          <button
            type="button"
            class="px-3 py-1.5 rounded-lg border cursor-pointer"
            :class="
              store.isDark
                ? 'border-slate-700 text-slate-300'
                : 'border-slate-300 text-slate-600'
            "
            @click="showAddRoleModal = false"
          >
            取消
          </button>
          <button
            type="button"
            class="px-4 py-1.5 rounded-lg bg-indigo-600 text-white font-semibold cursor-pointer hover:bg-indigo-500"
            @click="saveNewCustomRole"
          >
            保存角色
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
