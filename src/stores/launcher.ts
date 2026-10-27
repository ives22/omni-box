import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import { evaluate } from 'mathjs'
import {
  SearchResultItem,
  SystemAppItem,
  TotpEntry,
  ExternalUtoolsPlugin,
  UserSettings,
  DEFAULT_USER_SETTINGS,
  queryLauncherItems,
  fetchSystemApps,
  triggerRescanSystemApps,
  searchSpotlightLocalFiles,
  revealPathInFinder,
  runSystemCommand,
  generateTotpCode,
  quickTranslateEnglish,
  kvGet,
  kvPut,
  recordUsage,
  getFrecencyMap,
  installUtoolsGlobalShim,
  isTauriEnv
} from '@/bridge/backend'
import { invoke } from '@tauri-apps/api/core'

const DEFAULT_TOTP_LIST: TotpEntry[] = [
  {
    id: 'totp-github',
    issuer: 'GitHub',
    account: 'dev@omnibox.io',
    secret: 'JBSWY3DPEHPK3PXP',
    period: 30,
    digits: 6,
    category: '开发平台'
  },
  {
    id: 'totp-aws',
    issuer: 'AWS Root Cloud',
    account: 'admin@aws-prod.internal',
    secret: 'KRUGS4ZANFZSAYJA',
    period: 30,
    digits: 6,
    category: '云服务'
  },
  {
    id: 'totp-google',
    issuer: 'Google Workspace',
    account: 'liyj@workspace.org',
    secret: 'MFRGGZDFMZTWQ2LK',
    period: 30,
    digits: 6,
    category: '办公套件'
  },
  {
    id: 'totp-cloudflare',
    issuer: 'Cloudflare ZeroTrust',
    account: 'secops@omnibox.io',
    secret: 'GEZDGNBVGY3TQOJQ',
    period: 30,
    digits: 6,
    category: '基础设施'
  }
]

const DEFAULT_EXTERNAL_PLUGINS: ExternalUtoolsPlugin[] = [
  {
    id: 'ext:regex-playground',
    pluginName: '正则可视化测试 (uTools 兼容插件)',
    description: '实时匹配高亮正则表达式，内置常用邮箱、IP、身份证、URL 正则模板库',
    version: '1.2.0',
    author: 'uTools Community',
    main: 'index.html',
    cmds: ['regex', '正则', 'zz', 'regexp'],
    installedAt: Date.now() - 86400000
  },
  {
    id: 'ext:timestamp-converter',
    pluginName: 'Unix 时间戳转换器 (uTools 兼容插件)',
    description: '毫秒/秒级 Unix 时间戳与 ISO8601 / 北京时间双向极速转换',
    version: '1.0.4',
    author: 'Rubick Ecosystem',
    main: 'index.html',
    cmds: ['time', 'timestamp', '时间戳', 'sjc'],
    installedAt: Date.now() - 3600000
  }
]

export const useLauncherStore = defineStore('launcher', () => {
  const urlParams =
    typeof window !== 'undefined' ? new URLSearchParams(window.location.search) : null
  const standaloneParam = urlParams?.get('standalone') || null
  const standaloneExtParam = urlParams?.get('ext') || null

  const searchQuery = ref('')
  const subInputQuery = ref('')
  const subInputPlaceholder = ref('在插件内检索或过滤...')
  const activePluginId = ref<string | null>(standaloneParam)
  const isStandaloneWindow = ref(Boolean(standaloneParam))
  const isWindowMaximized = ref(false)
  const isDetachedWindow = ref(Boolean(standaloneParam))
  const isPinnedOnTop = ref(false)
  const selectedIndex = ref(0)
  const isDark = ref(true)
  const results = ref<SearchResultItem[]>([])
  const searchLatencyMs = ref<number>(0.15)

  const pluginInitialPayload = ref<string>('')
  const isSuperPanelOpen = ref(false)
  const superPanelClipboard = ref<string>('')
  const superPanelTranslation = ref<string | null>(null)
  const aiAnswerState = ref<{ question: string; answer: string; loading: boolean } | null>(null)

  const toastMessage = ref<string | null>(null)
  const toastType = ref<'success' | 'info' | 'warning'>('success')
  let toastTimer: ReturnType<typeof setTimeout> | null = null
  let fileSearchTimer: ReturnType<typeof setTimeout> | null = null
  let currentSearchSeq = 0

  // Persistent Data & System Apps
  const systemApps = ref<SystemAppItem[]>([])
  const totpEntries = ref<TotpEntry[]>([])
  const totpLiveMap = ref<Record<string, { code: string; remaining: number }>>({})
  const externalPlugins = ref<ExternalUtoolsPlugin[]>([])
  const activeExternalPlugin = ref<ExternalUtoolsPlugin | null>(null)
  const userSettings = ref<UserSettings>({ ...DEFAULT_USER_SETTINGS })
  const usageStatsMap = ref<Record<string, number>>({})

  function showToast(msg: string, type: 'success' | 'info' | 'warning' = 'success') {
    toastMessage.value = msg
    toastType.value = type
    if (toastTimer) clearTimeout(toastTimer)
    toastTimer = setTimeout(() => {
      toastMessage.value = null
    }, 2400)
  }

  function applyDarkClass(dark: boolean) {
    isDark.value = dark
    const root = document.documentElement
    if (dark) {
      root.classList.add('dark')
      root.classList.remove('light')
    } else {
      root.classList.remove('dark')
      root.classList.add('light')
    }
    kvPut('system', 'theme-dark', dark)
  }

  function toggleTheme() {
    const nextDark = !isDark.value
    applyDarkClass(nextDark)
    userSettings.value.themeMode = nextDark ? 'dark' : 'light'
    kvPut('system', 'user-settings', userSettings.value)
  }

  function setThemeMode(mode: 'system' | 'dark' | 'light') {
    userSettings.value.themeMode = mode
    if (mode === 'dark') {
      applyDarkClass(true)
    } else if (mode === 'light') {
      applyDarkClass(false)
    } else {
      const sysDark =
        typeof window !== 'undefined' &&
        window.matchMedia &&
        window.matchMedia('(prefers-color-scheme: dark)').matches
      applyDarkClass(sysDark)
    }
    kvPut('system', 'user-settings', userSettings.value)
  }

  function syncPreferencesToNative() {
    if (!isTauriEnv()) return
    const s = userSettings.value
    invoke('sync_native_preferences', {
      hideOnBlur: s.hideOnBlur !== false,
      detached: isDetachedWindow.value,
      superPanelEnabled: s.enableSuperPanel !== false,
      superPanelHoldMs: s.superPanelHoldMs ?? 250,
      superPanelTrigger: s.superPanelMouseTrigger || '长按鼠标右键',
      windowOpacity: s.windowOpacity ?? 98,
      launchAtLogin: s.launchAtLogin !== false
    }).catch(() => {})
  }

  async function saveUserSettings(next?: Partial<UserSettings>) {
    if (next) {
      userSettings.value = { ...userSettings.value, ...next }
    }
    if (isTauriEnv() && userSettings.value.globalHotkey) {
      invoke('update_global_hotkey', { hotkey: userSettings.value.globalHotkey }).catch(() => {})
    }
    syncPreferencesToNative()
    await kvPut('system', 'user-settings', userSettings.value)
    await runSearch()
  }

  async function openSuperPanel(clipText: string) {
    activePluginId.value = null
    isSuperPanelOpen.value = true
    const clean = (clipText || '').trim()
    superPanelClipboard.value = clean
    superPanelTranslation.value = null
    if (clean && userSettings.value.quickTranslate !== false) {
      superPanelTranslation.value = await quickTranslateEnglish(clean)
    }
  }

  async function triggerAiQuickAsk(question: string) {
    const cleanQ = question.replace(/^(\?|？|ai\s+)/i, '').trim()
    if (!cleanQ) return
    aiAnswerState.value = {
      question: cleanQ,
      answer: '正在连接 AI 模型思考中...',
      loading: true
    }

    const provider = userSettings.value.aiProvider || 'Google Gemini'
    const apiKey = (userSettings.value.aiApiKey || '').trim()
    const baseUrl = (userSettings.value.aiBaseUrl || '').trim().replace(/\/+$/, '')
    const model = (userSettings.value.aiModelName || 'gemini-2.5-pro').trim()

    if (!apiKey && provider !== 'Ollama 本地模型') {
      aiAnswerState.value = {
        question: cleanQ,
        answer: `[${provider} · ${model}]\n尚未配置 API Key。请前往「设置 -> AI 模型」填写您的 API Key（或切换为 Ollama 本地模型），即可在搜索框享受直达 AI 问答！`,
        loading: false
      }
      return
    }

    try {
      if (provider === 'Google Gemini') {
        const host = baseUrl || 'https://generativelanguage.googleapis.com'
        const url = `${host}/v1beta/models/${model}:generateContent?key=${encodeURIComponent(apiKey)}`
        const res = await fetch(url, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            contents: [{ parts: [{ text: cleanQ }] }]
          })
        })
        const data = await res.json()
        const text =
          data?.candidates?.[0]?.content?.parts?.[0]?.text ||
          data?.error?.message ||
          '未获取到有效响应'
        aiAnswerState.value = { question: cleanQ, answer: text, loading: false }
      } else {
        const host =
          baseUrl ||
          (provider === 'DeepSeek'
            ? 'https://api.deepseek.com/v1'
            : provider === 'Ollama 本地模型'
              ? 'http://127.0.0.1:11434/v1'
              : 'https://api.openai.com/v1')
        const url = `${host.endsWith('/chat/completions') ? host : `${host}/chat/completions`}`
        const headers: Record<string, string> = { 'Content-Type': 'application/json' }
        if (apiKey) headers['Authorization'] = `Bearer ${apiKey}`
        const res = await fetch(url, {
          method: 'POST',
          headers,
          body: JSON.stringify({
            model,
            messages: [{ role: 'user', content: cleanQ }]
          })
        })
        const data = await res.json()
        const text =
          data?.choices?.[0]?.message?.content || data?.error?.message || '未获取到有效响应'
        aiAnswerState.value = { question: cleanQ, answer: text, loading: false }
      }
    } catch (e) {
      aiAnswerState.value = {
        question: cleanQ,
        answer: `请求 AI 接口失败: ${String(e)}`,
        loading: false
      }
    }
  }

  async function refreshSystemApps(clearIconCache = false) {
    if (clearIconCache) {
      systemApps.value = await triggerRescanSystemApps(true)
    } else {
      systemApps.value = await fetchSystemApps()
    }
    await runSearch()
  }

  async function refreshTotpCodes() {
    const map: Record<string, { code: string; remaining: number }> = {}
    for (const item of totpEntries.value) {
      try {
        map[item.id] = await generateTotpCode(item.secret, item.period, item.digits)
      } catch {
        map[item.id] = { code: '------', remaining: 30 }
      }
    }
    totpLiveMap.value = map
  }

  function tryEvalQuickMath(q: string): SearchResultItem | null {
    const trimmed = q.trim()
    if (!trimmed || trimmed.length < 2) return null
    if (!/[0-9]/.test(trimmed) || !/[+\-*/^%()=]|(\bto\b|\bin\b|\bsin\b|\bcos\b|\bsqrt\b|\blog\b)/i.test(trimmed)) {
      return null
    }
    try {
      const res = evaluate(trimmed)
      if (res !== undefined && res !== null && typeof res !== 'function') {
        const formatted = typeof res === 'object' ? res.toString() : String(res)
        if (formatted && formatted !== 'undefined' && formatted !== 'NaN' && formatted !== trimmed) {
          return {
            id: 'quick:math',
            title: `= ${formatted}`,
            subtitle: `算式实时求值：${trimmed} （按 Enter 复制结果，按 Tab 送入计算稿纸）`,
            category: 'calc',
            icon: 'Calculator',
            keywords: [],
            action_payload: formatted,
            score: 9999,
            match_type: '实时计算',
            badge: '算式直达'
          }
        }
      }
    } catch {
      // ignore
    }
    return null
  }

  function tryDetectQuickJson(q: string): SearchResultItem | null {
    if (!userSettings.value.smartClipboard) return null
    const trimmed = q.trim()
    if (
      (trimmed.startsWith('{') && trimmed.endsWith('}')) ||
      (trimmed.startsWith('[') && trimmed.endsWith(']'))
    ) {
      try {
        JSON.parse(trimmed)
        return {
          id: 'quick:json',
          title: '在 JSON 助手中打开并美化此 JSON',
          subtitle: `已识别合法 JSON 结构 (${trimmed.length} 字符) — 按 Enter 立即格式化与分析`,
          category: 'clipboard',
          icon: 'Braces',
          keywords: [],
          action_payload: trimmed,
          score: 9998,
          match_type: '智能感知',
          badge: 'JSON 感知'
        }
      } catch {
        // invalid json
      }
    }
    return null
  }

  async function runSearch() {
    const t0 = performance.now()
    const q = searchQuery.value
    const trimmed = q.trim()
    const seq = ++currentSearchSeq
    if (fileSearchTimer) {
      clearTimeout(fileSearchTimer)
      fileSearchTimer = null
    }

    // Check if user is entering Spotlight Local File Search mode (`f` or `f <query>` or `find <query>`)
    if (/^(f|find)(\s+.*)?$/i.test(trimmed)) {
      const hasSpace = /^(f|find)\s+/i.test(q)
      if (hasSpace) {
        const fileQuery = trimmed.replace(/^(f|find)\s*/i, '').trim()
        if (!fileQuery) {
          results.value = [
            {
              id: 'file-mode:hint',
              title: 'Spotlight 本地文件聚焦搜索模式',
              subtitle: '请继续输入文件名关键词（如：f Cargo.toml / f package.json / f report）',
              category: 'file',
              icon: 'FolderGit2',
              keywords: [],
              action_payload: '',
              score: 1000,
              match_type: '文件检索模式',
              badge: 'Spotlight'
            }
          ]
          selectedIndex.value = 0
          searchLatencyMs.value = 0.1
          return
        }

        // Debounce 100ms so rapid typing never spawns redundant mdfind processes
        fileSearchTimer = setTimeout(async () => {
          const ft0 = performance.now()
          const fileItems = await searchSpotlightLocalFiles(fileQuery)
          if (seq !== currentSearchSeq) return
          results.value = fileItems
          selectedIndex.value = 0
          searchLatencyMs.value = Number(Math.max(0.2, performance.now() - ft0).toFixed(2))
        }, 100)
        return
      }
    }

    const dynamicItems: Omit<SearchResultItem, 'score' | 'match_type'>[] = []

    // 1. Add scanned local macOS applications (excluding blacklisted apps)
    const disabledSet = new Set(userSettings.value.disabledAppIds || [])
    for (const app of systemApps.value) {
      if (disabledSet.has(app.id)) continue
      dynamicItems.push({
        id: app.id,
        title: app.title,
        subtitle: app.subtitle,
        category: 'app',
        icon: app.icon || 'AppWindow',
        icon_data_url: app.icon_data_url || null,
        keywords: app.keywords,
        action_payload: app.action_payload,
        badge: '本机应用'
      })
    }

    // 2. Add 2FA direct items if enabled in settings
    if (userSettings.value.directTotpInSearch) {
      for (const totp of totpEntries.value) {
        const live = totpLiveMap.value[totp.id] || { code: '......', remaining: 30 }
        dynamicItems.push({
          id: `totp-direct:${totp.id}`,
          title: `${totp.issuer} (${totp.account})`,
          subtitle: `2FA 动态验证码 • 剩余 ${live.remaining}s 刷新 • 按 Enter 直接复制验证码`,
          category: 'totp',
          icon: 'ShieldCheck',
          keywords: ['2fa', 'totp', 'otp', totp.issuer.toLowerCase(), totp.account.toLowerCase(), '验证码'],
          action_payload: live.code,
          badge: '2FA 直达',
          totpCode: live.code,
          totpRemaining: live.remaining
        })
      }
    }

    // 3. Add external uTools plugins
    for (const ext of externalPlugins.value) {
      dynamicItems.push({
        id: ext.id,
        title: ext.pluginName,
        subtitle: `${ext.description} (v${ext.version} · 关键词: ${ext.cmds.join(' / ')})`,
        category: 'plugin',
        icon: 'Blocks',
        keywords: [...ext.cmds, ext.pluginName],
        action_payload: ext.id,
        badge: 'uTools 插件'
      })
    }

    const matched = await queryLauncherItems(
      q,
      dynamicItems,
      userSettings.value.aliases,
      userSettings.value.maxResultsCount,
      userSettings.value.showAppRecommendation !== false
    )
    const finalResults: SearchResultItem[] = []

    const quickJson = tryDetectQuickJson(q)
    if (quickJson) finalResults.push(quickJson)

    const quickMath = tryEvalQuickMath(q)
    if (quickMath) finalResults.push(quickMath)

    // Quick English -> Chinese Translation (when enabled in Super Panel / Preferences)
    if (userSettings.value.quickTranslate !== false && trimmed.length >= 2) {
      const translated = await quickTranslateEnglish(trimmed)
      if (seq !== currentSearchSeq) return
      if (translated) {
        finalResults.push({
          id: 'quick:translate',
          title: `译：${translated}`,
          subtitle: `英文快速翻译 (${trimmed}) — 按 Enter 立即复制中文释义`,
          category: 'clipboard',
          icon: 'Globe',
          keywords: [],
          action_payload: translated,
          score: 9997,
          match_type: '英文速译',
          badge: '快速翻译'
        })
      }
    }

    finalResults.push(...matched)

    // AI Quick Ask Item (when AI Quick Ask is enabled and user prefixes with ? / ai or no strict match)
    if (
      userSettings.value.aiQuickAskEnabled !== false &&
      trimmed.length >= 2 &&
      (/^[?？]|^ai\s+/i.test(trimmed) || finalResults.length <= 2)
    ) {
      const cleanPrompt = trimmed.replace(/^([?？]|ai\s+)/i, '').trim() || trimmed
      const provider = userSettings.value.aiProvider || 'Google Gemini'
      finalResults.push({
        id: 'quick:ai-ask',
        title: `向 ${provider} 提问：${cleanPrompt}`,
        subtitle: `使用 ${userSettings.value.aiModelName || 'AI 模型'} 即时解答此问题（按 Enter 发送）`,
        category: 'command',
        icon: 'Sparkles',
        keywords: [],
        action_payload: cleanPrompt,
        score: /^[?？]|^ai\s+/i.test(trimmed) ? 9996 : 100,
        match_type: 'AI 问答',
        badge: 'AI 助手'
      })
    }

    results.value = finalResults
    selectedIndex.value = 0
    searchLatencyMs.value = Number(Math.max(0.08, performance.now() - t0).toFixed(2))
  }

  async function executeItem(item: SearchResultItem, revealOnly = false) {
    if (item.id === 'file-mode:hint' || !item.action_payload) {
      return
    }
    await recordUsage(item.id)
    usageStatsMap.value = getFrecencyMap()

    // Cmd + Enter on app or file -> Reveal in macOS Finder and close launcher window immediately
    if (revealOnly && (item.category === 'app' || item.category === 'file')) {
      const targetPath = item.action_payload
      searchQuery.value = ''
      toastMessage.value = null
      isDetachedWindow.value = false
      isWindowMaximized.value = false
      await hideAppWindow()
      await revealPathInFinder(targetPath)
      return
    }

    if (item.id === 'quick:math' || item.id === 'quick:translate') {
      await navigator.clipboard.writeText(item.action_payload)
      searchQuery.value = ''
      await hideAppWindow()
      return
    }

    if (item.id === 'quick:ai-ask') {
      await triggerAiQuickAsk(item.action_payload)
      return
    }

    if (item.id === 'quick:json') {
      pluginInitialPayload.value = item.action_payload
      openPlugin('json-studio')
      searchQuery.value = ''
      return
    }

    if (item.category === 'totp' && item.totpCode) {
      await navigator.clipboard.writeText(item.totpCode)
      showToast(`已复制 ${item.title} 验证码：${item.totpCode}`)
      return
    }

    if (item.category === 'command') {
      try {
        const msg = await runSystemCommand(item.action_payload)
        if (item.action_payload === 'sys:copy-local-ip') {
          await navigator.clipboard.writeText(msg)
          showToast(`已复制本机局域网 IP：${msg}`)
        } else {
          showToast(msg)
        }
      } catch (e) {
        showToast(`命令执行失败：${String(e)}`, 'warning')
      }
      return
    }

    if (item.category === 'app' || item.category === 'file') {
      const targetPath = item.action_payload
      searchQuery.value = ''
      toastMessage.value = null
      isDetachedWindow.value = false
      isWindowMaximized.value = false
      if (isTauriEnv()) {
        try {
          await hideAppWindow()
          await invoke('launch_system_app', { appPath: targetPath })
        } catch {
          // silently ignore
        }
      }
      return
    }

    if (item.id.startsWith('ext:')) {
      const found = externalPlugins.value.find((p) => p.id === item.id)
      if (found) {
        activeExternalPlugin.value = found
        openPlugin('utools-runner')
      }
      return
    }

    if (item.category === 'plugin') {
      openPlugin(item.action_payload)
    }
  }

  function openPlugin(pluginId: string) {
    activePluginId.value = pluginId
    subInputQuery.value = ''
    const metaMap: Record<string, string> = {
      'calc-pad': '输入稿纸标题或搜索算式变量...',
      'json-studio': '输入 JSONPath 表达式过滤 (如 $.users[0] 或 .items)...',
      'diff-viewer': '在对比视图中高亮检索关键词...',
      'authenticator': '按平台名称、账户邮箱或分类筛选 2FA 令牌...',
      'password-vault': '搜索密码本中的网站、账号、备注...',
      'utools-hub': '搜索已挂载的 uTools 扩展或输入 plugin.json 配置...',
      'utools-runner': 'uTools 兼容沙箱子输入框 (window.utools.setSubInput)...',
      'user-center': '在个人中心搜索本机应用或配置项...'
    }
    subInputPlaceholder.value = metaMap[pluginId] || '在插件内检索...'
  }

  function closePlugin() {
    if (isStandaloneWindow.value) {
      closeCurrentWindow()
      return
    }
    activePluginId.value = null
    activeExternalPlugin.value = null
    isWindowMaximized.value = false
    isDetachedWindow.value = false
    subInputQuery.value = ''
    runSearch()
  }

  function closeCurrentWindow() {
    if (isTauriEnv()) {
      invoke('close_current_window').catch(() => {})
    } else {
      activePluginId.value = null
      isDetachedWindow.value = false
      isWindowMaximized.value = false
    }
  }

  async function detachCurrentPluginToWindow() {
    if (isStandaloneWindow.value) {
      showToast('当前已是独立窗口，点击其它应用不会自动关闭', 'info')
      return
    }

    let targetPluginId = activePluginId.value
    let targetExtId = activeExternalPlugin.value?.id || ''

    // If in main launcher search list and a plugin is selected, allow detaching that selected plugin directly
    if (!targetPluginId) {
      const current = results.value[selectedIndex.value]
      if (current && current.category === 'plugin') {
        if (current.id.startsWith('ext:')) {
          const found = externalPlugins.value.find((p) => p.id === current.id)
          if (found) {
            targetPluginId = 'utools-runner'
            targetExtId = found.id
          }
        } else {
          targetPluginId = current.action_payload
        }
      }
    }

    if (targetPluginId) {
      const titleMap: Record<string, string> = {
        'calc-pad': '计算稿纸',
        'json-studio': 'JSON 助手',
        'diff-viewer': '文本对比',
        'authenticator': '2FA 验证码',
        'password-vault': '密码本',
        'utools-hub': 'uTools 插件中心',
        'user-center': '个人中心与设置'
      }
      const title =
        activePluginMeta.value?.name || titleMap[targetPluginId] || 'OmniBox 独立窗口'

      try {
        if (subInputQuery.value) {
          localStorage.setItem(
            `omnibox:standalone-subinput:${targetPluginId}`,
            subInputQuery.value
          )
        }
        if (pluginInitialPayload.value) {
          localStorage.setItem(
            `omnibox:standalone-payload:${targetPluginId}`,
            pluginInitialPayload.value
          )
        }
      } catch {
        // ignore storage errors
      }

      if (isTauriEnv()) {
        try {
          await invoke('spawn_detached_plugin_window', {
            pluginId: targetPluginId,
            extId: targetExtId || null,
            title
          })
          // Reset main launcher state cleanly after spawning standalone window
          activePluginId.value = null
          activeExternalPlugin.value = null
          subInputQuery.value = ''
          isDetachedWindow.value = false
          isWindowMaximized.value = false
          return
        } catch {
          // Fallback to in-window detach if window creation fails
        }
      }

      isDetachedWindow.value = true
      isPinnedOnTop.value = false
      showToast(`已将「${title}」切换为独立窗口`, 'info')
      return
    }

    // Fallback when in main search bar with no plugin open
    isDetachedWindow.value = !isDetachedWindow.value
    showToast(
      isDetachedWindow.value
        ? '已设为独立窗口模式（点击其它应用不会自动关闭）'
        : '已恢复浮动搜索框模式',
      'info'
    )
  }

  function toggleWindowMaximize() {
    isWindowMaximized.value = !isWindowMaximized.value
  }

  function minimizeAppWindow() {
    if (isTauriEnv()) {
      invoke('minimize_window').catch(() => {})
    }
  }

  function hideAppWindow() {
    if (isTauriEnv()) {
      invoke('hide_window').catch(() => {})
    }
  }

  function startDraggingWindow() {
    if (isTauriEnv()) {
      invoke('start_window_drag').catch(() => {})
    }
  }

  function togglePinOnTop() {
    isPinnedOnTop.value = !isPinnedOnTop.value
    if (isTauriEnv()) {
      invoke('set_window_always_on_top', { pinned: isPinnedOnTop.value }).catch(() => {})
    }
    showToast(isPinnedOnTop.value ? '已开启窗口置顶' : '已取消窗口置顶', 'info')
  }

  async function saveTotpEntries(list: TotpEntry[]) {
    totpEntries.value = list
    await kvPut('authenticator', 'entries', list)
    await refreshTotpCodes()
    await runSearch()
  }

  async function saveExternalPlugins(list: ExternalUtoolsPlugin[]) {
    externalPlugins.value = list
    await kvPut('utools-hub', 'plugins', list)
    await runSearch()
  }

  const activePluginMeta = computed(() => {
    if (!activePluginId.value) return null
    if (activePluginId.value === 'utools-runner' && activeExternalPlugin.value) {
      return {
        id: 'utools-runner',
        name: activeExternalPlugin.value.pluginName,
        icon: 'Blocks',
        badge: 'uTools 沙箱'
      }
    }
    const map: Record<string, { id: string; name: string; icon: string; badge: string }> = {
      'calc-pad': { id: 'calc-pad', name: '计算稿纸', icon: 'Calculator', badge: 'Calc Pad' },
      'json-studio': { id: 'json-studio', name: 'JSON 助手', icon: 'Braces', badge: 'JSON Studio' },
      'diff-viewer': { id: 'diff-viewer', name: '文本对比', icon: 'GitCompare', badge: 'Diff Viewer' },
      'authenticator': { id: 'authenticator', name: '2FA 验证码', icon: 'ShieldCheck', badge: 'TOTP 2FA' },
      'password-vault': { id: 'password-vault', name: '密码本', icon: 'KeyRound', badge: 'AES-256 Vault' },
      'utools-hub': { id: 'utools-hub', name: 'uTools 插件中心', icon: 'Blocks', badge: '无限制挂载' },
      'user-center': { id: 'user-center', name: '个人中心与设置', icon: 'SlidersHorizontal', badge: 'Control Hub' }
    }
    return map[activePluginId.value] || null
  })

  async function initStore() {
    installUtoolsGlobalShim((copied) => {
      showToast(`已复制到剪贴板：${copied.slice(0, 32)}`)
    })

    const savedSettings = await kvGet<UserSettings>('system', 'user-settings', DEFAULT_USER_SETTINGS)
    userSettings.value = {
      ...DEFAULT_USER_SETTINGS,
      ...savedSettings,
      vaultAutoLockMinutes: savedSettings?.vaultAutoLockMinutes ?? DEFAULT_USER_SETTINGS.vaultAutoLockMinutes ?? 15,
      vaultClipboardClearSeconds:
        savedSettings?.vaultClipboardClearSeconds ?? DEFAULT_USER_SETTINGS.vaultClipboardClearSeconds ?? 30
    }

    // Apply themeMode (system / dark / light) and listen for OS appearance changes
    if (userSettings.value.themeMode) {
      setThemeMode(userSettings.value.themeMode)
    } else {
      const savedDark = await kvGet<boolean>('system', 'theme-dark', false)
      applyDarkClass(savedDark)
    }

    if (typeof window !== 'undefined' && window.matchMedia) {
      window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
        if (userSettings.value.themeMode === 'system') {
          applyDarkClass(e.matches)
        }
      })
    }

    if (isTauriEnv() && userSettings.value.globalHotkey) {
      invoke('update_global_hotkey', { hotkey: userSettings.value.globalHotkey }).catch(() => {})
    }
    syncPreferencesToNative()
    usageStatsMap.value = getFrecencyMap()

    totpEntries.value = await kvGet<TotpEntry[]>('authenticator', 'entries', DEFAULT_TOTP_LIST)
    externalPlugins.value = await kvGet<ExternalUtoolsPlugin[]>(
      'utools-hub',
      'plugins',
      DEFAULT_EXTERNAL_PLUGINS
    )

    if (standaloneParam) {
      openPlugin(standaloneParam)
      if (standaloneExtParam) {
        const foundExt = externalPlugins.value.find((p) => p.id === standaloneExtParam)
        if (foundExt) {
          activeExternalPlugin.value = foundExt
        }
      }
      try {
        const savedSub = localStorage.getItem(`omnibox:standalone-subinput:${standaloneParam}`)
        if (savedSub) {
          subInputQuery.value = savedSub
          localStorage.removeItem(`omnibox:standalone-subinput:${standaloneParam}`)
        }
        const savedPayload = localStorage.getItem(`omnibox:standalone-payload:${standaloneParam}`)
        if (savedPayload) {
          pluginInitialPayload.value = savedPayload
          localStorage.removeItem(`omnibox:standalone-payload:${standaloneParam}`)
        }
      } catch {
        // ignore
      }
    }

    systemApps.value = await fetchSystemApps()
    await refreshTotpCodes()
    await runSearch()

    setInterval(async () => {
      await refreshTotpCodes()
      if (!activePluginId.value && searchQuery.value.trim()) {
        for (const r of results.value) {
          if (r.category === 'totp') {
            const rawId = r.id.replace('totp-direct:', '')
            const live = totpLiveMap.value[rawId]
            if (live) {
              r.totpCode = live.code
              r.totpRemaining = live.remaining
              r.action_payload = live.code
              r.subtitle = `2FA 动态验证码 • 剩余 ${live.remaining}s 刷新 • 按 Enter 直接复制验证码`
            }
          }
        }
      }
    }, 1000)
  }

  watch(isDetachedWindow, () => {
    syncPreferencesToNative()
  })

  watch(searchQuery, () => {
    aiAnswerState.value = null
    if (!activePluginId.value) {
      runSearch()
    }
  })

  return {
    searchQuery,
    subInputQuery,
    subInputPlaceholder,
    activePluginId,
    activePluginMeta,
    isStandaloneWindow,
    isWindowMaximized,
    isDetachedWindow,
    isPinnedOnTop,
    selectedIndex,
    isDark,
    results,
    searchLatencyMs,
    pluginInitialPayload,
    isSuperPanelOpen,
    superPanelClipboard,
    superPanelTranslation,
    aiAnswerState,
    toastMessage,
    toastType,
    systemApps,
    totpEntries,
    totpLiveMap,
    externalPlugins,
    activeExternalPlugin,
    userSettings,
    usageStatsMap,
    showToast,
    toggleTheme,
    setThemeMode,
    saveUserSettings,
    syncPreferencesToNative,
    openSuperPanel,
    triggerAiQuickAsk,
    refreshSystemApps,
    runSearch,
    executeItem,
    openPlugin,
    closePlugin,
    closeCurrentWindow,
    detachCurrentPluginToWindow,
    toggleWindowMaximize,
    minimizeAppWindow,
    hideAppWindow,
    startDraggingWindow,
    togglePinOnTop,
    saveTotpEntries,
    saveExternalPlugins,
    initStore
  }
})
