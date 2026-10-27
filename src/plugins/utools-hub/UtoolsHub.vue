<script setup lang="ts">
import { ref, computed } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import { ExternalUtoolsPlugin } from '@/bridge/backend'
import IconRenderer from '@/components/IconRenderer.vue'

const store = useLauncherStore()

const showManifestModal = ref(false)
const manifestJsonInput = ref(
  JSON.stringify(
    {
      pluginName: 'Base64 & URL 编解码器',
      description: '兼容 uTools API 的极速 Base64、URL、Hex 双向编码转换工具',
      version: '1.1.0',
      author: 'Local Dev',
      main: 'index.html',
      features: [
        {
          code: 'codec',
          explain: '编码解码转换',
          cmds: ['base64', 'url', '编解码', 'bjm']
        }
      ]
    },
    null,
    2
  )
)

const regexPattern = ref('([a-zA-Z0-9._-]+@[a-zA-Z0-9._-]+\\.[a-zA-Z0-9_-]+)')
const regexFlags = ref('g')
const regexTestStr = ref(
  '联系邮箱：architect@omnibox.io，备用支持：support_2026@google.com，无效地址：admin@localhost'
)
const tsInput = ref(String(Math.floor(Date.now() / 1000)))

const regexMatches = computed(() => {
  try {
    const re = new RegExp(regexPattern.value, regexFlags.value)
    const list: string[] = []
    let m: RegExpExecArray | null
    while ((m = re.exec(regexTestStr.value)) !== null) {
      list.push(m[0])
      if (!regexFlags.value.includes('g')) break
    }
    return { ok: true, matches: list, error: null }
  } catch (e) {
    return { ok: false, matches: [], error: (e as Error).message }
  }
})

const convertedTime = computed(() => {
  const num = Number(tsInput.value.trim())
  if (Number.isNaN(num)) return '无效的时间戳数字'
  const ms = num < 1e11 ? num * 1000 : num
  const d = new Date(ms)
  return {
    iso: d.toISOString(),
    local: d.toLocaleString('zh-CN', { hour12: false }),
    utc: d.toUTCString()
  }
})

async function installFromManifest() {
  try {
    const parsed = JSON.parse(manifestJsonInput.value)
    const cmds: string[] = []
    if (Array.isArray(parsed.features)) {
      for (const f of parsed.features) {
        if (Array.isArray(f.cmds)) cmds.push(...f.cmds)
      }
    }
    const newPlugin: ExternalUtoolsPlugin = {
      id: `ext:${Date.now()}`,
      pluginName: parsed.pluginName || '未命名 uTools 插件',
      description: parsed.description || '本地挂载的第三方 uTools 兼容扩展',
      version: parsed.version || '1.0.0',
      author: parsed.author || 'Community',
      main: parsed.main || 'index.html',
      cmds: cmds.length > 0 ? cmds : ['plugin'],
      installedAt: Date.now()
    }
    await store.saveExternalPlugins([newPlugin, ...store.externalPlugins])
    showManifestModal.value = false
    store.showToast(`已成功挂载插件：${newPlugin.pluginName}`)
  } catch {
    store.showToast('plugin.json 格式解析失败，请检查 JSON 语法', 'warning')
  }
}

function launchExternal(plugin: ExternalUtoolsPlugin) {
  store.activeExternalPlugin = plugin
  store.openPlugin('utools-runner')
}

async function removeExternal(id: string) {
  const next = store.externalPlugins.filter((p) => p.id !== id)
  await store.saveExternalPlugins(next)
  store.showToast('已卸载该外部扩展', 'info')
}
</script>

<template>
  <div
    class="flex flex-col flex-1 h-full min-h-[500px] select-none"
    :class="store.isDark ? 'bg-[#0E131F]' : 'bg-[#F8FAFC]'"
  >
    <!-- Mode A: Running inside an External uTools Plugin Sandbox -->
    <template v-if="store.activePluginId === 'utools-runner' && store.activeExternalPlugin">
      <div
        class="flex items-center justify-between px-4 py-2 border-b text-xs"
        :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800' : 'bg-slate-50 border-slate-200'"
      >
        <div class="flex items-center gap-2">
          <span
            class="px-2 py-0.5 rounded-md font-mono border"
            :class="
              store.isDark
                ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
                : 'bg-indigo-50 text-indigo-600 border-indigo-200/80'
            "
          >
            window.utools 兼容沙箱已注入
          </span>
          <span class="text-slate-400">
            入口: <code class="font-mono">{{ store.activeExternalPlugin.main }}</code> • 版本 v{{ store.activeExternalPlugin.version }}
          </span>
        </div>
        <button
          type="button"
          class="btn-secondary !px-2.5 !py-1"
          @click="store.openPlugin('utools-hub')"
        >
          返回插件中心
        </button>
      </div>

      <!-- Sandbox Content for Regex or Timestamp or Generic uTools Plugin -->
      <div
        class="flex-1 p-5 overflow-y-auto space-y-4"
        :class="store.isDark ? 'bg-transparent' : 'bg-[#F8FAFC]'"
      >
        <div
          v-if="store.activeExternalPlugin.id === 'ext:regex-playground'"
          class="space-y-4"
        >
          <div class="flex items-center gap-2">
            <span class="font-mono text-slate-400">/</span>
            <input
              v-model="regexPattern"
              type="text"
              aria-label="正则表达式输入"
              class="flex-1 px-3 py-2 rounded-xl font-mono text-sm border outline-none text-indigo-500"
              :class="store.isDark ? 'bg-obsidian-950 border-slate-700' : 'bg-white border-slate-200/90'"
            />
            <span class="font-mono text-slate-400">/</span>
            <input
              v-model="regexFlags"
              type="text"
              aria-label="正则修饰符"
              class="w-16 px-2.5 py-2 rounded-xl font-mono text-sm border outline-none"
              :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-white' : 'bg-white border-slate-200/90'"
            />
          </div>

          <div>
            <label class="block text-xs text-slate-400 mb-1.5">待匹配测试文本</label>
            <textarea
              v-model="regexTestStr"
              rows="4"
              aria-label="正则待匹配测试文本"
              class="w-full p-3 rounded-xl font-mono text-xs border outline-none"
              :class="store.isDark ? 'bg-obsidian-950 border-slate-800 text-slate-200' : 'bg-white border-slate-200/90'"
            ></textarea>
          </div>

          <div
            class="p-3.5 rounded-xl border"
            :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800' : 'bg-white border-slate-200/80'"
          >
            <div class="text-xs font-semibold text-slate-400 mb-2">
              匹配结果 ({{ regexMatches.matches.length }} 处命中)
            </div>
            <div class="flex flex-wrap gap-2">
              <span
                v-for="(m, idx) in regexMatches.matches"
                :key="idx"
                class="px-2.5 py-1 rounded-lg font-mono text-xs border"
                :class="
                  store.isDark
                    ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
                    : 'bg-indigo-50 text-indigo-600 border-indigo-200/80'
                "
              >
                #{{ idx + 1 }}: {{ m }}
              </span>
            </div>
          </div>
        </div>

        <div v-else class="space-y-4">
          <div class="flex items-center gap-3">
            <label class="text-xs text-slate-400">Unix 时间戳 (秒/毫秒):</label>
            <input
              v-model="tsInput"
              type="text"
              aria-label="时间戳输入"
              class="px-3 py-1.5 rounded-xl font-mono text-sm border outline-none text-indigo-500"
              :class="store.isDark ? 'bg-obsidian-950 border-slate-700' : 'bg-white border-slate-200/90'"
            />
            <button
              type="button"
              class="btn-secondary"
              @click="tsInput = String(Math.floor(Date.now() / 1000))"
            >
              当前时间
            </button>
          </div>

          <div
            v-if="typeof convertedTime === 'object'"
            class="grid grid-cols-1 gap-2.5 font-mono text-xs"
          >
            <div
              v-for="(val, label) in {
                '本地时间 (Asia/Shanghai)': convertedTime.local,
                'ISO 8601 标准格式': convertedTime.iso,
                'UTC 世界标准时间': convertedTime.utc
              }"
              :key="label"
              class="flex items-center justify-between p-3 rounded-xl border"
              :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800' : 'bg-white border-slate-200/80'"
            >
              <div>
                <div class="text-slate-400 text-[11px]">{{ label }}</div>
                <div
                  class="text-sm mt-0.5"
                  :class="store.isDark ? 'text-indigo-300' : 'text-indigo-600'"
                >
                  {{ val }}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>

    <!-- Mode B: Plugin Hub & Manifest Loader -->
    <template v-else>
      <div
        class="flex items-center justify-between px-4 py-2.5 border-b text-xs"
        :class="store.isDark ? 'bg-obsidian-950/60 border-slate-800' : 'bg-slate-50/90 border-slate-200/80'"
      >
        <div class="flex items-center gap-2 text-slate-400">
          <IconRenderer name="Blocks" :size="15" class-name="text-indigo-500" />
          <span>
            完全兼容 uTools / Rubick 的
            <code
              class="font-mono px-1 py-0.5 rounded"
              :class="store.isDark ? 'bg-indigo-500/15 text-indigo-300' : 'bg-indigo-50 text-indigo-600'"
            >
              plugin.json
            </code>
            标准，无 10 个应用安装上限
          </span>
        </div>
        <button
          type="button"
          class="btn-primary"
          @click="showManifestModal = !showManifestModal"
        >
          <IconRenderer :name="showManifestModal ? 'X' : 'Plus'" :size="14" />
          <span>{{ showManifestModal ? '收起导入面板' : '挂载本地 plugin.json 插件' }}</span>
        </button>
      </div>

      <div
        v-if="showManifestModal"
        class="p-4 border-b space-y-2.5"
        :class="store.isDark ? 'bg-slate-900/95 border-slate-800' : 'bg-slate-100/80 border-slate-200'"
      >
        <div class="flex items-center justify-between text-xs">
          <span class="text-slate-400">
            粘贴第三方 uTools 插件的 <code class="font-mono text-indigo-500">plugin.json</code> 清单即可注册到全局搜索：
          </span>
          <button
            type="button"
            class="btn-primary"
            @click="installFromManifest"
          >
            立即解析并挂载
          </button>
        </div>
        <textarea
          v-model="manifestJsonInput"
          rows="5"
          spellcheck="false"
          aria-label="plugin.json 清单配置输入"
          class="w-full p-2.5 rounded-xl font-mono text-xs border outline-none"
          :class="store.isDark ? 'bg-obsidian-950 border-slate-700 text-indigo-200' : 'bg-white border-slate-300'"
        ></textarea>
      </div>

      <div
        class="flex-1 overflow-y-auto p-4 space-y-2.5"
        :class="store.isDark ? 'bg-transparent' : 'bg-[#F8FAFC]'"
      >
        <div
          v-for="ext in store.externalPlugins"
          :key="ext.id"
          class="flex items-center justify-between p-4 rounded-2xl border transition-all"
          :class="
            store.isDark
              ? 'bg-obsidian-800/60 border-slate-800 hover:border-slate-700'
              : 'bg-white border-slate-200/80 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
          "
        >
          <div class="space-y-1">
            <div class="flex items-center gap-2">
              <span class="text-sm font-bold" :class="store.isDark ? 'text-white' : 'text-slate-900'">
                {{ ext.pluginName }}
              </span>
              <span
                class="px-1.5 py-0.5 text-[10px] font-mono rounded-md border"
                :class="
                  store.isDark
                    ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
                    : 'bg-indigo-50 text-indigo-600 border-indigo-200/80'
                "
              >
                v{{ ext.version }}
              </span>
            </div>
            <p class="text-xs text-slate-400">{{ ext.description }}</p>
            <div class="flex items-center gap-1.5 pt-1">
              <span class="text-[11px] text-slate-400">触发指令:</span>
              <span
                v-for="cmd in ext.cmds"
                :key="cmd"
                class="kbd-badge"
              >
                {{ cmd }}
              </span>
            </div>
          </div>

          <div class="flex items-center gap-2">
            <button
              type="button"
              class="btn-primary"
              @click="launchExternal(ext)"
            >
              <IconRenderer name="Play" :size="13" />
              <span>运行插件</span>
            </button>
            <button
              type="button"
              aria-label="卸载此插件"
              class="p-1.5 rounded-lg text-slate-400 hover:text-rose-500 hover:bg-rose-500/10 cursor-pointer"
              @click="removeExternal(ext.id)"
            >
              <IconRenderer name="Trash2" :size="14" />
            </button>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
