<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import {
  LocalNetworkOverview,
  PublicIpGeoInfo,
  fetchLocalNetworkOverview,
  fetchPublicIpGeo
} from '@/bridge/backend'
import IconRenderer from '@/components/IconRenderer.vue'

const store = useLauncherStore()

type StudioTab = 'overview' | 'cidr'
const activeTab = ref<StudioTab>('overview')

// ============================================================================
// 1. In-Place Copy Feedback State (Matches PasswordVault Dual-Feedback UX)
// ============================================================================
const lastCopiedKey = ref<string | null>(null)
let copyFeedbackTimer: ReturnType<typeof setTimeout> | null = null

async function copyField(val: string | undefined | null, label: string, keyId: string) {
  const clean = (val || '').trim()
  if (!clean) return
  try {
    await navigator.clipboard.writeText(clean)
    lastCopiedKey.value = keyId
    if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer)
    copyFeedbackTimer = setTimeout(() => {
      if (lastCopiedKey.value === keyId) {
        lastCopiedKey.value = null
      }
    }, 1350)
    store.showToast(`已复制${label}：${clean}`)
  } catch {
    // ignore
  }
}

function isCopied(keyId: string): boolean {
  return lastCopiedKey.value === keyId
}

// ============================================================================
// 2. Tab 1: Local Multi-NIC Overview & Public Exit IP Intelligence
// ============================================================================
const localNet = ref<LocalNetworkOverview | null>(null)
const isLoadingLocal = ref(false)

const publicGeo = ref<PublicIpGeoInfo | null>(null)
const isLoadingPublic = ref(false)
const publicError = ref<string | null>(null)
const selectedGeoSource = ref<'auto' | 'geojs' | 'ipapi' | 'ipwhois'>('auto')

// Domestic Direct Exit Comparison (IPIP.net)
const domesticDirectGeo = ref<PublicIpGeoInfo | null>(null)
const isLoadingDomestic = ref(false)

// Custom 3rd-party IP Lookup
const customLookupIp = ref('')
const customLookupResult = ref<PublicIpGeoInfo | null>(null)
const isLoadingCustomLookup = ref(false)
const customLookupError = ref<string | null>(null)

async function loadLocalNetwork() {
  isLoadingLocal.value = true
  try {
    const res = await fetchLocalNetworkOverview()
    localNet.value = res
    // If CIDR input is still default, sync with actual primary interface IP & prefix
    if (cidrIpInput.value === '192.168.1.100' && res.primary_ipv4 && res.primary_ipv4 !== '127.0.0.1') {
      cidrIpInput.value = res.primary_ipv4
      const primaryIface = res.interfaces.find((i) => i.is_primary) || res.interfaces[0]
      if (primaryIface && primaryIface.cidr_prefix) {
        cidrPrefix.value = primaryIface.cidr_prefix
      }
    }
  } finally {
    isLoadingLocal.value = false
  }
}

async function loadPublicExitIp() {
  isLoadingPublic.value = true
  publicError.value = null
  try {
    publicGeo.value = await fetchPublicIpGeo('', selectedGeoSource.value)
  } catch (e) {
    publicError.value = String(e)
  } finally {
    isLoadingPublic.value = false
  }
}

async function loadDomesticDirectIp() {
  isLoadingDomestic.value = true
  try {
    domesticDirectGeo.value = await fetchPublicIpGeo('', 'ipip')
  } catch {
    domesticDirectGeo.value = null
  } finally {
    isLoadingDomestic.value = false
  }
}

async function refreshAllNetworkData() {
  loadLocalNetwork()
  loadPublicExitIp()
  loadDomesticDirectIp()
}

async function handleCustomIpLookup(ipToQuery?: string) {
  const target = (ipToQuery ?? customLookupIp.value).trim()
  if (!target) {
    customLookupResult.value = null
    customLookupError.value = null
    return
  }
  customLookupIp.value = target
  isLoadingCustomLookup.value = true
  customLookupError.value = null
  try {
    customLookupResult.value = await fetchPublicIpGeo(target, selectedGeoSource.value)
  } catch (e) {
    customLookupError.value = `查询 ${target} 失败：${String(e)}`
  } finally {
    isLoadingCustomLookup.value = false
  }
}

const primaryInterfaceObj = computed(() => {
  if (!localNet.value || localNet.value.interfaces.length === 0) return null
  return localNet.value.interfaces.find((i) => i.is_primary) || localNet.value.interfaces[0]
})

// ============================================================================
// 3. Tab 2: IPv4 CIDR & VLSM Subnet Calculator Engine
// ============================================================================
const cidrIpInput = ref('192.168.1.100')
const cidrPrefix = ref<number>(24)
const splitTargetPrefix = ref<number>(26)
const testIpInCidr = ref('')

const quickPrefixPresets = [
  { prefix: 8, label: '/8', desc: 'A类' },
  { prefix: 16, label: '/16', desc: 'B类 65534台' },
  { prefix: 20, label: '/20', desc: '4094台' },
  { prefix: 22, label: '/22', desc: '1022台' },
  { prefix: 24, label: '/24', desc: 'C类 254台' },
  { prefix: 25, label: '/25', desc: '126台' },
  { prefix: 26, label: '/26', desc: '62台' },
  { prefix: 27, label: '/27', desc: '30台' },
  { prefix: 28, label: '/28', desc: '14台' },
  { prefix: 30, label: '/30', desc: '点对点 2台' },
  { prefix: 32, label: '/32', desc: '单主机' }
]

function ipv4ToU32(ip: string): number | null {
  const parts = ip.trim().split('.')
  if (parts.length !== 4) return null
  let acc = 0
  for (const p of parts) {
    if (!/^\d+$/.test(p)) return null
    const n = Number(p)
    if (n < 0 || n > 255) return null
    acc = ((acc << 8) | n) >>> 0
  }
  return acc >>> 0
}

function u32ToIpv4(val: number): string {
  const u = val >>> 0
  return `${(u >>> 24) & 0xff}.${(u >>> 16) & 0xff}.${(u >>> 8) & 0xff}.${u & 0xff}`
}

function prefixToMaskU32(prefix: number): number {
  if (prefix <= 0) return 0
  if (prefix >= 32) return 0xffffffff >>> 0
  return (0xffffffff << (32 - prefix)) >>> 0
}

function classifyIpv4Rfc(u32: number): { category: string; badge: string } {
  const b1 = (u32 >>> 24) & 0xff
  const b2 = (u32 >>> 16) & 0xff

  if (b1 === 10) {
    return { category: 'RFC 1918 私有局域网 (A 类大型内网 10.0.0.0/8)', badge: '私有内网 A类' }
  }
  if (b1 === 172 && b2 >= 16 && b2 <= 31) {
    return { category: 'RFC 1918 私有局域网 (B 类中型内网 172.16.0.0/12)', badge: '私有内网 B类' }
  }
  if (b1 === 192 && b2 === 168) {
    return { category: 'RFC 1918 私有局域网 (C 类常用内网 192.168.0.0/16)', badge: '私有内网 C类' }
  }
  if (b1 === 100 && b2 >= 64 && b2 <= 127) {
    return { category: 'RFC 6598 运营商级 NAT / Tailscale 组网网段 (100.64.0.0/10)', badge: 'CGNAT / SD-WAN' }
  }
  if (b1 === 127) {
    return { category: 'RFC 1122 本机环回测试地址 (Loopback 127.0.0.0/8)', badge: '本机环回' }
  }
  if (b1 === 169 && b2 === 254) {
    return { category: 'RFC 3927 链路本地自动配置地址 (APIPA 169.254.0.0/16)', badge: '链路本地' }
  }
  if (b1 >= 224 && b1 <= 239) {
    return { category: 'RFC 5771 组播地址段 (Multicast D类 224.0.0.0/4)', badge: 'D类组播' }
  }
  return { category: '全球公网可路由单播地址 (Public Routable Unicast)', badge: '公网单播' }
}

// Support typing `192.168.1.1/26` directly into the IP input box
function handleCidrCombinedInput(e: Event) {
  const raw = (e.target as HTMLInputElement).value.trim()
  if (raw.includes('/')) {
    const [ipPart, pfPart] = raw.split('/')
    if (ipPart) cidrIpInput.value = ipPart.trim()
    const pf = Number(pfPart)
    if (!Number.isNaN(pf) && pf >= 1 && pf <= 32) {
      cidrPrefix.value = pf
    }
  } else {
    cidrIpInput.value = raw
  }
}

const cidrAnalysis = computed(() => {
  const ipU32 = ipv4ToU32(cidrIpInput.value)
  if (ipU32 === null) return null

  const pf = Math.max(1, Math.min(32, cidrPrefix.value))
  const maskU32 = prefixToMaskU32(pf)
  const wildcardU32 = (~maskU32) >>> 0
  const networkU32 = (ipU32 & maskU32) >>> 0
  const broadcastU32 = (networkU32 | wildcardU32) >>> 0

  const totalAddresses = Math.pow(2, 32 - pf)
  let usableCount = 0
  let firstHostIp = ''
  let lastHostIp = ''

  if (pf === 32) {
    usableCount = 1
    firstHostIp = u32ToIpv4(networkU32)
    lastHostIp = u32ToIpv4(networkU32)
  } else if (pf === 31) {
    usableCount = 2
    firstHostIp = u32ToIpv4(networkU32)
    lastHostIp = u32ToIpv4(broadcastU32)
  } else {
    usableCount = Math.max(0, totalAddresses - 2)
    firstHostIp = u32ToIpv4((networkU32 + 1) >>> 0)
    lastHostIp = u32ToIpv4((broadcastU32 - 1) >>> 0)
  }

  // Build 32-bit binary representation grouped into 4 octets
  const bits32 = ipU32.toString(2).padStart(32, '0')
  const maskBits32 = maskU32.toString(2).padStart(32, '0')
  const octets = [
    bits32.slice(0, 8),
    bits32.slice(8, 16),
    bits32.slice(16, 24),
    bits32.slice(24, 32)
  ]

  const rfcInfo = classifyIpv4Rfc(ipU32)

  return {
    inputIp: u32ToIpv4(ipU32),
    prefix: pf,
    standardCidr: `${u32ToIpv4(networkU32)}/${pf}`,
    networkIp: u32ToIpv4(networkU32),
    broadcastIp: u32ToIpv4(broadcastU32),
    subnetMask: u32ToIpv4(maskU32),
    subnetMaskHex: `0x${maskU32.toString(16).padStart(8, '0')}`,
    wildcardMask: u32ToIpv4(wildcardU32),
    firstHostIp,
    lastHostIp,
    hostRange: `${firstHostIp} - ${lastHostIp}`,
    usableHosts: usableCount.toLocaleString('en-US'),
    totalAddresses: totalAddresses.toLocaleString('en-US'),
    octets,
    maskBits32,
    networkU32,
    broadcastU32,
    rfcCategory: rfcInfo.category,
    rfcBadge: rfcInfo.badge
  }
})

// Keep splitTargetPrefix valid when cidrPrefix changes
watch(cidrPrefix, (newPf) => {
  if (splitTargetPrefix.value <= newPf || splitTargetPrefix.value > Math.min(32, newPf + 6)) {
    splitTargetPrefix.value = Math.min(32, newPf + 2)
  }
})

const splitOptions = computed(() => {
  if (!cidrAnalysis.value) return []
  const curPf = cidrAnalysis.value.prefix
  const list: { targetPf: number; subnetCount: number; hostsPerSubnet: number }[] = []
  for (let p = curPf + 1; p <= Math.min(32, curPf + 5); p++) {
    const count = Math.pow(2, p - curPf)
    const totalPer = Math.pow(2, 32 - p)
    const hosts = p === 32 ? 1 : p === 31 ? 2 : Math.max(0, totalPer - 2)
    list.push({ targetPf: p, subnetCount: count, hostsPerSubnet: hosts })
  }
  return list
})

const splitSubnetsList = computed(() => {
  const base = cidrAnalysis.value
  if (!base || splitTargetPrefix.value <= base.prefix || splitTargetPrefix.value > 32) {
    return []
  }
  const targetPf = splitTargetPrefix.value
  const count = Math.min(32, Math.pow(2, targetPf - base.prefix))
  const blockSize = Math.pow(2, 32 - targetPf)

  const items: {
    index: number
    cidr: string
    network: string
    range: string
    broadcast: string
  }[] = []

  for (let i = 0; i < count; i++) {
    const netU32 = (base.networkU32 + i * blockSize) >>> 0
    const bcastU32 = (netU32 + blockSize - 1) >>> 0
    const first =
      targetPf >= 31 ? u32ToIpv4(netU32) : u32ToIpv4((netU32 + 1) >>> 0)
    const last =
      targetPf >= 31 ? u32ToIpv4(bcastU32) : u32ToIpv4((bcastU32 - 1) >>> 0)
    items.push({
      index: i + 1,
      cidr: `${u32ToIpv4(netU32)}/${targetPf}`,
      network: u32ToIpv4(netU32),
      range: `${first} ~ ${last}`,
      broadcast: u32ToIpv4(bcastU32)
    })
  }
  return items
})

const ipInCidrCheckResult = computed(() => {
  const base = cidrAnalysis.value
  const q = testIpInCidr.value.trim()
  if (!base || !q) return null
  const testU32 = ipv4ToU32(q)
  if (testU32 === null) {
    return { valid: false, inside: false, message: '请输入合法的 IPv4 地址（如 192.168.1.55）' }
  }
  const inside = testU32 >= base.networkU32 && testU32 <= base.broadcastU32
  return {
    valid: true,
    inside,
    message: inside
      ? `✓ 该 IP (${q}) 属于网段 ${base.standardCidr}`
      : `✕ 该 IP (${q}) 不属于网段 ${base.standardCidr}`
  }
})

function sendInterfaceToCidr(ipv4: string, prefix: number) {
  cidrIpInput.value = ipv4
  cidrPrefix.value = prefix || 24
  activeTab.value = 'cidr'
}

// ============================================================================
// 4. React to Initial Payload or Sub-Input Search from OmniBox Header
// ============================================================================
function applyExternalQuery(raw: string) {
  const clean = raw.trim()
  if (!clean) return
  if (clean.includes('/')) {
    const [ipPart, pfPart] = clean.split('/')
    if (ipPart && ipv4ToU32(ipPart.trim()) !== null) {
      cidrIpInput.value = ipPart.trim()
      const pf = Number(pfPart)
      if (!Number.isNaN(pf) && pf >= 1 && pf <= 32) {
        cidrPrefix.value = pf
      }
      activeTab.value = 'cidr'
      return
    }
  }
  if (ipv4ToU32(clean) !== null) {
    // Valid standalone IPv4: populate both CIDR calculator and custom IP geo lookup
    cidrIpInput.value = clean
    const u = ipv4ToU32(clean)!
    const rfc = classifyIpv4Rfc(u)
    if (rfc.badge.startsWith('私有内网')) {
      activeTab.value = 'cidr'
    } else {
      activeTab.value = 'overview'
      handleCustomIpLookup(clean)
    }
  }
}

watch(
  () => store.subInputQuery,
  (val) => {
    const trimmed = val.trim()
    if (trimmed.includes('/') || ipv4ToU32(trimmed) !== null) {
      applyExternalQuery(trimmed)
    }
  }
)

onMounted(() => {
  refreshAllNetworkData()
  if (store.pluginInitialPayload) {
    const payload = store.pluginInitialPayload
    store.pluginInitialPayload = ''
    applyExternalQuery(payload)
  }
})
</script>

<template>
  <div
    class="flex flex-col flex-1 h-full min-h-[480px] overflow-hidden select-none"
    :class="store.isDark ? 'bg-[#0E131F] text-slate-100' : 'bg-[#F8FAFC] text-slate-800'"
  >
    <!-- ===================================================================== -->
    <!-- Top Segmented Navigation Bar                                          -->
    <!-- ===================================================================== -->
    <div
      class="flex items-center justify-between px-5 py-2.5 border-b shrink-0"
      :class="
        store.isDark
          ? 'bg-[#0A0E17]/90 border-slate-800/90'
          : 'bg-white border-slate-200/85'
      "
    >
      <div class="flex items-center gap-1.5 p-1 rounded-xl border" :class="store.isDark ? 'bg-slate-900/90 border-slate-800' : 'bg-slate-100/90 border-slate-200/70'">
        <button
          type="button"
          class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold cursor-pointer transition-all"
          :class="
            activeTab === 'overview'
              ? store.isDark
                ? 'bg-indigo-500/20 text-indigo-300 shadow-xs'
                : 'bg-white text-indigo-600 shadow-xs'
              : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
          "
          @click="activeTab = 'overview'"
        >
          <IconRenderer name="Globe" :size="14" />
          <span>本机出口与公网 IP 情报</span>
        </button>

        <button
          type="button"
          class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold cursor-pointer transition-all"
          :class="
            activeTab === 'cidr'
              ? store.isDark
                ? 'bg-indigo-500/20 text-indigo-300 shadow-xs'
                : 'bg-white text-indigo-600 shadow-xs'
              : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
          "
          @click="activeTab = 'cidr'"
        >
          <IconRenderer name="SlidersHorizontal" :size="14" />
          <span>IP 网络地址划分 (CIDR / 子网计算器)</span>
        </button>
      </div>

      <!-- Right Toolbar Controls -->
      <div v-if="activeTab === 'overview'" class="flex items-center gap-2">
        <span class="text-[11px] text-slate-400">公网查询源:</span>
        <select
          v-model="selectedGeoSource"
          aria-label="选择公网 IP 查询源"
          class="h-[28px] px-2.5 rounded-lg border text-[11.5px] font-medium outline-none cursor-pointer"
          :class="
            store.isDark
              ? 'bg-slate-900 border-slate-700 text-slate-200'
              : 'bg-white border-slate-200 text-slate-700'
          "
          @change="loadPublicExitIp()"
        >
          <option value="auto">智能多源聚合 (GeoJS + 中文)</option>
          <option value="geojs">GeoJS (get.geojs.io)</option>
          <option value="ipapi">ip-api.com (中文归属地)</option>
          <option value="ipwhois">ipwho.is (含时区偏移)</option>
        </select>

        <button
          type="button"
          class="flex items-center gap-1.5 h-[28px] px-2.5 rounded-lg border text-xs font-medium cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-indigo-500/15 border-indigo-500/30 text-indigo-300 hover:bg-indigo-500/25'
              : 'bg-indigo-50 border-indigo-200/80 text-indigo-600 hover:bg-indigo-100'
          "
          @click="refreshAllNetworkData"
        >
          <IconRenderer
            name="RefreshCw"
            :size="12"
            :class-name="isLoadingLocal || isLoadingPublic ? 'animate-spin' : ''"
          />
          <span>刷新探测</span>
        </button>
      </div>

      <div v-else class="flex items-center gap-1.5 text-[11px]">
        <button
          v-if="primaryInterfaceObj"
          type="button"
          class="px-2.5 py-1 rounded-lg border font-mono cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-emerald-500/15 border-emerald-500/30 text-emerald-300 hover:bg-emerald-500/25'
              : 'bg-emerald-50 border-emerald-200 text-emerald-700 hover:bg-emerald-100'
          "
          @click="sendInterfaceToCidr(primaryInterfaceObj.ipv4, primaryInterfaceObj.cidr_prefix)"
        >
          填入本机网段 ({{ primaryInterfaceObj.cidr_notation }})
        </button>
        <button
          v-for="sample in ['10.24.0.0/16', '172.16.0.0/20', '192.168.10.0/24']"
          :key="sample"
          type="button"
          class="px-2 py-1 rounded-lg border font-mono cursor-pointer transition-colors"
          :class="
            store.isDark
              ? 'bg-slate-900 border-slate-800 text-slate-400 hover:text-slate-200'
              : 'bg-white border-slate-200 text-slate-600 hover:bg-slate-50'
          "
          @click="applyExternalQuery(sample)"
        >
          {{ sample }}
        </button>
      </div>
    </div>

    <!-- ===================================================================== -->
    <!-- TAB 1: 本机多网卡与公网出口全景看板 (Overview)                        -->
    <!-- ===================================================================== -->
    <div v-if="activeTab === 'overview'" class="flex-1 overflow-y-auto px-5 py-4 space-y-4">
      <!-- Top Dual Primary Hero Cards: [Left: Local Active Outlet] + [Right: Public Exit IP] -->
      <div class="grid grid-cols-2 gap-3.5">
        <!-- Card A: 本机实际出口局域网 IP (Wi-Fi / Ethernet) -->
        <div
          class="p-4 rounded-2xl border flex flex-col justify-between space-y-3"
          :class="
            store.isDark
              ? 'bg-[#121929]/90 border-slate-800/90'
              : 'bg-white border-slate-200/85 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
          "
        >
          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <span class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
                <span class="text-xs font-bold tracking-tight">本机实际出口局域网 IP</span>
              </div>
              <span
                v-if="primaryInterfaceObj"
                class="px-2 py-0.5 rounded-full text-[10.5px] font-medium border"
                :class="
                  store.isDark
                    ? 'bg-emerald-500/15 border-emerald-500/30 text-emerald-300'
                    : 'bg-emerald-50 border-emerald-200 text-emerald-700'
                "
              >
                {{ primaryInterfaceObj.display_name }}
              </span>
            </div>

            <!-- Main Local IPv4 Display + Copy Button -->
            <div
              class="flex items-center justify-between p-2.5 rounded-xl border transition-colors"
              :class="[
                isCopied('primary-local-ip')
                  ? 'border-emerald-500/70 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-950/70 border-slate-800/90'
                    : 'bg-slate-50/90 border-slate-200/75'
              ]"
            >
              <div class="min-w-0">
                <div class="text-[10.5px] text-slate-400">当前默认路由局域网 IPv4</div>
                <div class="font-mono text-xl font-bold tracking-tight mt-0.5 select-text">
                  {{ localNet?.primary_ipv4 || '正在探测...' }}
                </div>
              </div>
              <div class="flex items-center gap-1.5">
                <button
                  v-if="primaryInterfaceObj"
                  type="button"
                  title="送入 CIDR 子网计算器划分此网段"
                  class="px-2.5 py-1.5 rounded-lg border text-[11px] font-medium cursor-pointer transition-colors"
                  :class="
                    store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-300 hover:border-indigo-500/50 hover:text-indigo-300'
                      : 'bg-white border-slate-200 text-slate-600 hover:border-indigo-300 hover:text-indigo-600'
                  "
                  @click="sendInterfaceToCidr(primaryInterfaceObj.ipv4, primaryInterfaceObj.cidr_prefix)"
                >
                  子网划分
                </button>
                <button
                  type="button"
                  class="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-semibold border cursor-pointer transition-all"
                  :class="
                    isCopied('primary-local-ip')
                      ? 'bg-emerald-500 text-white border-emerald-500'
                      : store.isDark
                        ? 'bg-indigo-500/20 border-indigo-500/40 text-indigo-300 hover:bg-indigo-500/30'
                        : 'bg-indigo-600 text-white border-indigo-600 hover:bg-indigo-700'
                  "
                  @click="copyField(localNet?.primary_ipv4, '本机局域网出口 IP', 'primary-local-ip')"
                >
                  <IconRenderer :name="isCopied('primary-local-ip') ? 'Check' : 'Copy'" :size="13" />
                  <span>{{ isCopied('primary-local-ip') ? '已复制' : '复制 IP' }}</span>
                </button>
              </div>
            </div>
          </div>

          <!-- 2x2 Key Local Network Metrics -->
          <div class="grid grid-cols-2 gap-2 text-xs">
            <div
              class="p-2 rounded-xl border flex items-center justify-between gap-1.5 cursor-pointer transition-colors"
              :class="[
                isCopied('local-gateway')
                  ? 'border-emerald-500/60 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-900/60 border-slate-800/80 hover:border-slate-700'
                    : 'bg-slate-50/70 border-slate-200/70 hover:border-slate-300'
              ]"
              @click="copyField(localNet?.gateway || '', '默认网关 IP', 'local-gateway')"
            >
              <div class="min-w-0">
                <div class="text-[10px] text-slate-400">默认网关 (Gateway)</div>
                <div class="font-mono font-semibold truncate mt-0.5">
                  {{ localNet?.gateway || '未检测到' }}
                </div>
              </div>
              <IconRenderer
                :name="isCopied('local-gateway') ? 'Check' : 'Copy'"
                :size="12"
                :class-name="isCopied('local-gateway') ? 'text-emerald-400 shrink-0' : 'text-slate-400 shrink-0'"
              />
            </div>

            <div
              class="p-2 rounded-xl border flex items-center justify-between gap-1.5 cursor-pointer transition-colors"
              :class="[
                isCopied('local-netmask')
                  ? 'border-emerald-500/60 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-900/60 border-slate-800/80 hover:border-slate-700'
                    : 'bg-slate-50/70 border-slate-200/70 hover:border-slate-300'
              ]"
              @click="
                copyField(
                  primaryInterfaceObj
                    ? `${primaryInterfaceObj.netmask} (/${primaryInterfaceObj.cidr_prefix})`
                    : '',
                  '子网掩码',
                  'local-netmask'
                )
              "
            >
              <div class="min-w-0">
                <div class="text-[10px] text-slate-400">子网掩码 (Netmask)</div>
                <div class="font-mono font-semibold truncate mt-0.5">
                  {{
                    primaryInterfaceObj
                      ? `${primaryInterfaceObj.netmask} (/${primaryInterfaceObj.cidr_prefix})`
                      : '255.255.255.0 (/24)'
                  }}
                </div>
              </div>
              <IconRenderer
                :name="isCopied('local-netmask') ? 'Check' : 'Copy'"
                :size="12"
                :class-name="isCopied('local-netmask') ? 'text-emerald-400 shrink-0' : 'text-slate-400 shrink-0'"
              />
            </div>

            <div
              class="p-2 rounded-xl border flex items-center justify-between gap-1.5 cursor-pointer transition-colors"
              :class="[
                isCopied('local-dns')
                  ? 'border-emerald-500/60 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-900/60 border-slate-800/80 hover:border-slate-700'
                    : 'bg-slate-50/70 border-slate-200/70 hover:border-slate-300'
              ]"
              @click="copyField(localNet?.dns_servers?.join(', ') || '', 'DNS 服务器', 'local-dns')"
            >
              <div class="min-w-0">
                <div class="text-[10px] text-slate-400">当前 DNS 服务器</div>
                <div class="font-mono font-semibold truncate mt-0.5">
                  {{ localNet?.dns_servers?.length ? localNet.dns_servers.join(', ') : '系统自动' }}
                </div>
              </div>
              <IconRenderer
                :name="isCopied('local-dns') ? 'Check' : 'Copy'"
                :size="12"
                :class-name="isCopied('local-dns') ? 'text-emerald-400 shrink-0' : 'text-slate-400 shrink-0'"
              />
            </div>

            <div
              class="p-2 rounded-xl border flex items-center justify-between gap-1.5 cursor-pointer transition-colors"
              :class="[
                isCopied('local-mac')
                  ? 'border-emerald-500/60 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-900/60 border-slate-800/80 hover:border-slate-700'
                    : 'bg-slate-50/70 border-slate-200/70 hover:border-slate-300'
              ]"
              @click="copyField(primaryInterfaceObj?.mac || '', 'MAC 地址', 'local-mac')"
            >
              <div class="min-w-0">
                <div class="text-[10px] text-slate-400">物理地址 (MAC)</div>
                <div class="font-mono font-semibold truncate mt-0.5">
                  {{ primaryInterfaceObj?.mac || '无硬件地址' }}
                </div>
              </div>
              <IconRenderer
                :name="isCopied('local-mac') ? 'Check' : 'Copy'"
                :size="12"
                :class-name="isCopied('local-mac') ? 'text-emerald-400 shrink-0' : 'text-slate-400 shrink-0'"
              />
            </div>
          </div>
        </div>

        <!-- Card B: 公网出口 IP 情报 (GeoJS + ip-api + ipwho.is) -->
        <div
          class="p-4 rounded-2xl border flex flex-col justify-between space-y-3"
          :class="
            store.isDark
              ? 'bg-[#121929]/90 border-slate-800/90'
              : 'bg-white border-slate-200/85 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
          "
        >
          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <span class="w-2 h-2 rounded-full bg-indigo-400"></span>
                <span class="text-xs font-bold tracking-tight">公网出口 IP 与地理/时区情报</span>
              </div>
              <span
                v-if="publicGeo"
                class="px-2 py-0.5 rounded-full text-[10px] font-mono border"
                :class="
                  store.isDark
                    ? 'bg-indigo-500/15 border-indigo-500/30 text-indigo-300'
                    : 'bg-indigo-50 border-indigo-200 text-indigo-600'
                "
              >
                {{ publicGeo.sourceName }} · {{ publicGeo.latencyMs }}ms
              </span>
            </div>

            <!-- Main Public IPv4 Display + Copy Button -->
            <div
              class="flex items-center justify-between p-2.5 rounded-xl border transition-colors"
              :class="[
                isCopied('public-exit-ip')
                  ? 'border-emerald-500/70 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-950/70 border-slate-800/90'
                    : 'bg-slate-50/90 border-slate-200/75'
              ]"
            >
              <div class="min-w-0">
                <div class="text-[10.5px] text-slate-400">当前网络公网出口 IP</div>
                <div class="flex items-center gap-2 mt-0.5">
                  <span v-if="publicGeo?.flagEmoji" class="text-lg leading-none">
                    {{ publicGeo.flagEmoji }}
                  </span>
                  <span class="font-mono text-xl font-bold tracking-tight select-text">
                    {{
                      isLoadingPublic
                        ? '正在查询出口 IP...'
                        : publicGeo?.ip || publicError || '未获取到公网 IP'
                    }}
                  </span>
                </div>
              </div>

              <button
                v-if="publicGeo?.ip"
                type="button"
                class="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-semibold border cursor-pointer transition-all shrink-0"
                :class="
                  isCopied('public-exit-ip')
                    ? 'bg-emerald-500 text-white border-emerald-500'
                    : store.isDark
                      ? 'bg-indigo-500/20 border-indigo-500/40 text-indigo-300 hover:bg-indigo-500/30'
                      : 'bg-indigo-600 text-white border-indigo-600 hover:bg-indigo-700'
                "
                @click="copyField(publicGeo?.ip, '公网出口 IP', 'public-exit-ip')"
              >
                <IconRenderer :name="isCopied('public-exit-ip') ? 'Check' : 'Copy'" :size="13" />
                <span>{{ isCopied('public-exit-ip') ? '已复制' : '复制 IP' }}</span>
              </button>
            </div>
          </div>

          <!-- 2x2 Public IP Geo Details (Location / Timezone / ASN / Split-Tunnel Domestic) -->
          <div class="grid grid-cols-2 gap-2 text-xs">
            <div
              class="p-2 rounded-xl border flex items-center justify-between gap-1.5 cursor-pointer transition-colors"
              :class="[
                isCopied('public-loc')
                  ? 'border-emerald-500/60 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-900/60 border-slate-800/80 hover:border-slate-700'
                    : 'bg-slate-50/70 border-slate-200/70 hover:border-slate-300'
              ]"
              @click="
                copyField(
                  publicGeo
                    ? [publicGeo.country, publicGeo.region, publicGeo.city].filter(Boolean).join(' ')
                    : '',
                  '出口地理位置',
                  'public-loc'
                )
              "
            >
              <div class="min-w-0">
                <div class="text-[10px] text-slate-400">出口归属地 (国家 / 城市)</div>
                <div class="font-semibold truncate mt-0.5">
                  {{
                    publicGeo
                      ? [publicGeo.country, publicGeo.region, publicGeo.city]
                          .filter(Boolean)
                          .join(' · ')
                      : '--'
                  }}
                </div>
              </div>
              <IconRenderer
                :name="isCopied('public-loc') ? 'Check' : 'Copy'"
                :size="12"
                :class-name="isCopied('public-loc') ? 'text-emerald-400 shrink-0' : 'text-slate-400 shrink-0'"
              />
            </div>

            <div
              class="p-2 rounded-xl border flex items-center justify-between gap-1.5 cursor-pointer transition-colors"
              :class="[
                isCopied('public-tz')
                  ? 'border-emerald-500/60 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-900/60 border-slate-800/80 hover:border-slate-700'
                    : 'bg-slate-50/70 border-slate-200/70 hover:border-slate-300'
              ]"
              @click="
                copyField(
                  publicGeo
                    ? `${publicGeo.timezone}${publicGeo.utcOffset ? ` (${publicGeo.utcOffset})` : ''}`
                    : '',
                  '出口时区',
                  'public-tz'
                )
              "
            >
              <div class="min-w-0">
                <div class="text-[10px] text-slate-400">出口时区 (Timezone)</div>
                <div class="font-mono font-semibold truncate mt-0.5">
                  {{
                    publicGeo
                      ? `${publicGeo.timezone}${publicGeo.utcOffset ? ` (${publicGeo.utcOffset})` : ''}`
                      : '--'
                  }}
                </div>
              </div>
              <IconRenderer
                :name="isCopied('public-tz') ? 'Check' : 'Copy'"
                :size="12"
                :class-name="isCopied('public-tz') ? 'text-emerald-400 shrink-0' : 'text-slate-400 shrink-0'"
              />
            </div>

            <div
              class="p-2 rounded-xl border flex items-center justify-between gap-1.5 cursor-pointer transition-colors"
              :class="[
                isCopied('public-asn')
                  ? 'border-emerald-500/60 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-900/60 border-slate-800/80 hover:border-slate-700'
                    : 'bg-slate-50/70 border-slate-200/70 hover:border-slate-300'
              ]"
              @click="
                copyField(
                  publicGeo ? `${publicGeo.asn} ${publicGeo.isp}`.trim() : '',
                  'ASN 与运营商',
                  'public-asn'
                )
              "
            >
              <div class="min-w-0">
                <div class="text-[10px] text-slate-400">运营商与自治域 (ISP / ASN)</div>
                <div class="font-mono font-semibold truncate mt-0.5">
                  {{
                    publicGeo
                      ? [publicGeo.asn, publicGeo.isp].filter(Boolean).join(' · ')
                      : '--'
                  }}
                </div>
              </div>
              <IconRenderer
                :name="isCopied('public-asn') ? 'Check' : 'Copy'"
                :size="12"
                :class-name="isCopied('public-asn') ? 'text-emerald-400 shrink-0' : 'text-slate-400 shrink-0'"
              />
            </div>

            <div
              class="p-2 rounded-xl border flex items-center justify-between gap-1.5 cursor-pointer transition-colors"
              :class="[
                isCopied('domestic-ip')
                  ? 'border-emerald-500/60 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-900/60 border-slate-800/80 hover:border-slate-700'
                    : 'bg-slate-50/70 border-slate-200/70 hover:border-slate-300'
              ]"
              @click="
                copyField(
                  domesticDirectGeo?.ip || publicGeo?.ip || '',
                  '国内检测源出口 IP',
                  'domestic-ip'
                )
              "
            >
              <div class="min-w-0">
                <div class="text-[10px] text-slate-400">国内检测源对比 (IPIP.net)</div>
                <div class="font-mono font-semibold truncate mt-0.5">
                  <template v-if="isLoadingDomestic">检测中...</template>
                  <template v-else-if="domesticDirectGeo">
                    {{ domesticDirectGeo.ip }} ({{ domesticDirectGeo.country }}{{ domesticDirectGeo.region }})
                  </template>
                  <template v-else>
                    {{ publicGeo ? `${publicGeo.latitude}, ${publicGeo.longitude}` : '--' }}
                  </template>
                </div>
              </div>
              <IconRenderer
                :name="isCopied('domestic-ip') ? 'Check' : 'Copy'"
                :size="12"
                :class-name="isCopied('domestic-ip') ? 'text-emerald-400 shrink-0' : 'text-slate-400 shrink-0'"
              />
            </div>
          </div>
        </div>
      </div>

      <!-- Middle Bar: Lookup Any 3rd-Party Public IP -->
      <div
        class="p-3.5 rounded-2xl border space-y-2.5"
        :class="
          store.isDark
            ? 'bg-[#121929]/80 border-slate-800/90'
            : 'bg-white border-slate-200/85 shadow-[0_1px_2px_rgba(15,23,42,0.02)]'
        "
      >
        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2 shrink-0">
            <IconRenderer name="Search" :size="14" class-name="text-indigo-400" />
            <span class="text-xs font-bold">查询任意指定公网 IP 归属地 / 时区 / ASN</span>
          </div>

          <div class="flex items-center gap-2 flex-1 max-w-[440px]">
            <input
              v-model="customLookupIp"
              type="text"
              placeholder="输入任意公网 IPv4 / IPv6 (如 8.8.8.8 / 1.1.1.1 / 223.5.5.5)..."
              aria-label="输入待查询的公网 IP"
              class="flex-1 h-[30px] px-3 rounded-lg border font-mono text-xs outline-none"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-700 text-slate-100 focus:border-indigo-500'
                  : 'bg-slate-50 border-slate-200 text-slate-800 focus:border-indigo-500'
              "
              @keydown.enter="handleCustomIpLookup()"
            />
            <button
              type="button"
              class="h-[30px] px-3 rounded-lg text-xs font-semibold cursor-pointer transition-colors shrink-0"
              :class="
                store.isDark
                  ? 'bg-indigo-500 text-white hover:bg-indigo-400'
                  : 'bg-indigo-600 text-white hover:bg-indigo-700'
              "
              @click="handleCustomIpLookup()"
            >
              {{ isLoadingCustomLookup ? '查询中...' : '立即查询' }}
            </button>
          </div>
        </div>

        <div v-if="customLookupError" class="text-xs text-rose-400">
          {{ customLookupError }}
        </div>

        <div
          v-else-if="customLookupResult"
          class="p-2.5 rounded-xl border flex items-center justify-between gap-3 text-xs"
          :class="
            store.isDark
              ? 'bg-slate-950/70 border-indigo-500/30 text-slate-200'
              : 'bg-indigo-50/50 border-indigo-200/80 text-slate-800'
          "
        >
          <div class="flex items-center gap-3 min-w-0 flex-wrap">
            <span class="font-mono font-bold text-sm text-indigo-400">
              {{ customLookupResult.flagEmoji }} {{ customLookupResult.ip }}
            </span>
            <span>
              归属地:
              <strong>
                {{
                  [customLookupResult.country, customLookupResult.region, customLookupResult.city]
                    .filter(Boolean)
                    .join(' · ')
                }}
              </strong>
            </span>
            <span class="font-mono">
              ASN/ISP: <strong>{{ customLookupResult.asn }} {{ customLookupResult.isp }}</strong>
            </span>
            <span class="font-mono">
              时区: <strong>{{ customLookupResult.timezone }}</strong>
            </span>
          </div>

          <button
            type="button"
            class="px-2.5 py-1 rounded-lg border text-[11px] font-medium shrink-0 cursor-pointer"
            :class="
              store.isDark
                ? 'bg-slate-900 border-slate-700 text-slate-300 hover:text-white'
                : 'bg-white border-slate-200 text-slate-700 hover:bg-slate-50'
            "
            @click="
              copyField(
                `${customLookupResult.ip} | ${customLookupResult.country} ${customLookupResult.region} ${customLookupResult.city} | ${customLookupResult.asn} ${customLookupResult.isp} | ${customLookupResult.timezone}`,
                'IP 情报摘要',
                'custom-ip-summary'
              )
            "
          >
            {{ isCopied('custom-ip-summary') ? '✓ 已复制摘要' : '复制完整情报' }}
          </button>
        </div>
      </div>

      <!-- Bottom Section: All Active Network Interfaces List (Wi-Fi / Ethernet / VPN / Bridge) -->
      <div class="space-y-2">
        <div class="flex items-center justify-between px-1">
          <div class="text-xs font-bold">
            本机活动网卡接口明细 ({{ localNet?.interfaces?.length || 0 }} 个活动 IPv4 接口)
          </div>
          <div class="text-[11px] text-slate-400 font-mono">
            主机名: {{ localNet?.hostname || 'Mac' }}
          </div>
        </div>

        <div class="grid grid-cols-2 gap-2.5">
          <div
            v-for="iface in localNet?.interfaces || []"
            :key="iface.name"
            class="p-3 rounded-xl border flex items-center justify-between gap-3 text-xs transition-all"
            :class="[
              iface.is_primary
                ? store.isDark
                  ? 'bg-emerald-500/8 border-emerald-500/35'
                  : 'bg-emerald-50/40 border-emerald-300/80'
                : store.isDark
                  ? 'bg-[#121929]/75 border-slate-800/90'
                  : 'bg-white border-slate-200/85'
            ]"
          >
            <div class="min-w-0 space-y-1">
              <div class="flex items-center gap-1.5">
                <span class="font-semibold truncate">{{ iface.display_name }}</span>
                <span
                  v-if="iface.is_primary"
                  class="px-1.5 py-0.2 rounded text-[10px] font-semibold bg-emerald-500/20 text-emerald-400 shrink-0"
                >
                  默认出口
                </span>
              </div>
              <div class="font-mono text-sm font-bold text-indigo-400 select-text">
                {{ iface.ipv4 }}
                <span class="text-xs font-normal text-slate-400">/{{ iface.cidr_prefix }}</span>
              </div>
              <div class="text-[10.5px] text-slate-400 font-mono truncate">
                掩码: {{ iface.netmask }}
                <template v-if="iface.mac"> · MAC: {{ iface.mac }}</template>
              </div>
            </div>

            <div class="flex items-center gap-1.5 shrink-0">
              <button
                type="button"
                title="送入子网计算器"
                class="px-2 py-1 rounded-lg border text-[11px] cursor-pointer transition-colors"
                :class="
                  store.isDark
                    ? 'bg-slate-900 border-slate-700 text-slate-300 hover:border-indigo-500/50'
                    : 'bg-slate-50 border-slate-200 text-slate-600 hover:border-indigo-300'
                "
                @click="sendInterfaceToCidr(iface.ipv4, iface.cidr_prefix)"
              >
                划分
              </button>
              <button
                type="button"
                class="flex items-center gap-1 px-2.5 py-1 rounded-lg border text-[11px] font-medium cursor-pointer transition-all"
                :class="
                  isCopied(`iface-${iface.name}`)
                    ? 'bg-emerald-500 text-white border-emerald-500'
                    : store.isDark
                      ? 'bg-slate-900 border-slate-700 text-slate-200 hover:border-indigo-500'
                      : 'bg-white border-slate-200 text-slate-700 hover:border-indigo-400'
                "
                @click="copyField(iface.ipv4, `${iface.display_name} IP`, `iface-${iface.name}`)"
              >
                <IconRenderer :name="isCopied(`iface-${iface.name}`) ? 'Check' : 'Copy'" :size="11" />
                <span>{{ isCopied(`iface-${iface.name}`) ? '已复制' : '复制' }}</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- ===================================================================== -->
    <!-- TAB 2: IP 网络地址划分与 CIDR 子网计算器 (CIDR & VLSM Calculator)     -->
    <!-- ===================================================================== -->
    <div v-else class="flex-1 overflow-y-auto px-5 py-4 space-y-4">
      <!-- Top Control Card: IPv4 + Prefix Slider + Preset Pills -->
      <div
        class="p-4 rounded-2xl border space-y-3"
        :class="
          store.isDark
            ? 'bg-[#121929]/90 border-slate-800/90'
            : 'bg-white border-slate-200/85 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
        "
      >
        <div class="flex items-center justify-between gap-4">
          <div class="flex items-center gap-2 flex-1">
            <div class="relative flex-1 max-w-[260px]">
              <input
                :value="cidrIpInput"
                type="text"
                placeholder="输入 IPv4 或 CIDR (如 192.168.1.0/24)"
                aria-label="目标 IPv4 地址"
                class="w-full h-[36px] px-3 rounded-xl border font-mono text-sm font-bold outline-none"
                :class="
                  cidrAnalysis
                    ? store.isDark
                      ? 'bg-slate-950 border-slate-700 text-white focus:border-indigo-500'
                      : 'bg-slate-50 border-slate-300 text-slate-900 focus:border-indigo-500'
                    : 'border-rose-500 bg-rose-500/10 text-rose-400'
                "
                @input="handleCidrCombinedInput"
              />
            </div>

            <span class="font-mono text-lg font-bold text-slate-400">/</span>

            <select
              v-model.number="cidrPrefix"
              aria-label="CIDR 掩码位数"
              class="h-[36px] px-3 rounded-xl border font-mono text-sm font-bold outline-none cursor-pointer"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-700 text-indigo-300'
                  : 'bg-slate-50 border-slate-300 text-indigo-600'
              "
            >
              <option v-for="p in 32" :key="p" :value="p">
                /{{ p }} ({{ u32ToIpv4(prefixToMaskU32(p)) }})
              </option>
            </select>

            <!-- Prefix Range Slider -->
            <div class="flex-1 flex items-center gap-2 pl-2">
              <span class="text-[11px] font-mono text-slate-400">/1</span>
              <input
                v-model.number="cidrPrefix"
                type="range"
                min="1"
                max="32"
                step="1"
                aria-label="拖动调整 CIDR 前缀长度"
                class="flex-1 accent-indigo-500 cursor-pointer"
              />
              <span class="text-[11px] font-mono text-slate-400">/32</span>
            </div>
          </div>

          <span
            v-if="cidrAnalysis"
            class="px-2.5 py-1 rounded-full text-[11px] font-semibold border shrink-0"
            :class="
              store.isDark
                ? 'bg-indigo-500/15 border-indigo-500/30 text-indigo-300'
                : 'bg-indigo-50 border-indigo-200 text-indigo-700'
            "
          >
            {{ cidrAnalysis.rfcBadge }}
          </span>
        </div>

        <!-- Quick Prefix Pills -->
        <div class="flex items-center gap-1.5 flex-wrap">
          <span class="text-[11px] text-slate-400 mr-1">常用掩码快选:</span>
          <button
            v-for="preset in quickPrefixPresets"
            :key="preset.prefix"
            type="button"
            class="px-2 py-0.5 rounded-lg border font-mono text-[11px] cursor-pointer transition-all"
            :class="
              cidrPrefix === preset.prefix
                ? store.isDark
                  ? 'bg-indigo-500/25 border-indigo-500 text-indigo-200 font-bold'
                  : 'bg-indigo-600 border-indigo-600 text-white font-bold'
                : store.isDark
                  ? 'bg-slate-900 border-slate-800 text-slate-400 hover:text-slate-200'
                  : 'bg-slate-50 border-slate-200 text-slate-600 hover:bg-slate-100'
            "
            @click="cidrPrefix = preset.prefix"
          >
            {{ preset.label }}
            <span class="text-[10px] opacity-75">{{ preset.desc }}</span>
          </button>
        </div>

        <!-- 32-Bit Binary Network vs Host Bitwise Visualizer -->
        <div
          v-if="cidrAnalysis"
          class="p-2.5 rounded-xl border flex items-center justify-between gap-3 font-mono text-xs"
          :class="
            store.isDark
              ? 'bg-slate-950/80 border-slate-800/90'
              : 'bg-slate-50 border-slate-200/80'
          "
        >
          <div class="flex items-center gap-1.5 flex-wrap">
            <span class="text-[10.5px] font-sans text-slate-400 mr-1">32位二进制位图:</span>
            <template v-for="(oct, oIdx) in cidrAnalysis.octets" :key="oIdx">
              <div class="inline-flex items-center tracking-wider">
                <span
                  v-for="(bitChar, bIdx) in oct.split('')"
                  :key="bIdx"
                  class="px-[1.5px] rounded-xs font-semibold"
                  :class="
                    oIdx * 8 + bIdx < cidrAnalysis.prefix
                      ? store.isDark
                        ? 'text-indigo-300 bg-indigo-500/15'
                        : 'text-indigo-700 bg-indigo-100/90'
                      : store.isDark
                        ? 'text-emerald-300 bg-emerald-500/15'
                        : 'text-emerald-700 bg-emerald-100/90'
                  "
                >
                  {{ bitChar }}
                </span>
              </div>
              <span v-if="oIdx < 3" class="text-slate-500 font-bold">.</span>
            </template>
          </div>

          <div class="flex items-center gap-3 text-[10.5px] font-sans shrink-0">
            <span class="flex items-center gap-1">
              <span class="w-2 h-2 rounded-xs bg-indigo-500"></span>
              <span class="text-slate-400">网络位 ({{ cidrAnalysis.prefix }} bit)</span>
            </span>
            <span class="flex items-center gap-1">
              <span class="w-2 h-2 rounded-xs bg-emerald-500"></span>
              <span class="text-slate-400">主机位 ({{ 32 - cidrAnalysis.prefix }} bit)</span>
            </span>
          </div>
        </div>
      </div>

      <!-- 8-Card CIDR Calculation Results Grid -->
      <div v-if="cidrAnalysis" class="grid grid-cols-4 gap-2.5 text-xs">
        <div
          v-for="card in [
            {
              key: 'cidr-std',
              label: '标准 CIDR 网段',
              value: cidrAnalysis.standardCidr,
              sub: cidrAnalysis.rfcCategory.split(' ')[0] + ' ' + (cidrAnalysis.rfcCategory.split(' ')[1] || '')
            },
            {
              key: 'cidr-net',
              label: '网络地址 (Network IP)',
              value: cidrAnalysis.networkIp,
              sub: '网段起始标识地址'
            },
            {
              key: 'cidr-bcast',
              label: '广播地址 (Broadcast IP)',
              value: cidrAnalysis.broadcastIp,
              sub: '网段末尾广播地址'
            },
            {
              key: 'cidr-hosts',
              label: '可用主机数量',
              value: `${cidrAnalysis.usableHosts} 台`,
              sub: `共 ${cidrAnalysis.totalAddresses} 个 IP 地址`
            },
            {
              key: 'cidr-range',
              label: '可用主机 IP 范围 (Host Range)',
              value: cidrAnalysis.hostRange,
              sub: `首: ${cidrAnalysis.firstHostIp} · 尾: ${cidrAnalysis.lastHostIp}`
            },
            {
              key: 'cidr-mask',
              label: '子网掩码 (Subnet Mask)',
              value: cidrAnalysis.subnetMask,
              sub: `十六进制: ${cidrAnalysis.subnetMaskHex}`
            },
            {
              key: 'cidr-wildcard',
              label: '反掩码 / 通配符 (Wildcard)',
              value: cidrAnalysis.wildcardMask,
              sub: 'ACL / OSPF 路由配置常用'
            },
            {
              key: 'cidr-rfc',
              label: '地址属性分类 (RFC)',
              value: cidrAnalysis.rfcBadge,
              sub: cidrAnalysis.rfcCategory
            }
          ]"
          :key="card.key"
          class="p-3 rounded-xl border flex flex-col justify-between cursor-pointer transition-all"
          :class="[
            isCopied(card.key)
              ? 'border-emerald-500/70 bg-emerald-500/10'
              : store.isDark
                ? 'bg-[#121929]/85 border-slate-800/90 hover:border-indigo-500/40'
                : 'bg-white border-slate-200/85 hover:border-indigo-300 shadow-[0_1px_2px_rgba(15,23,42,0.02)]'
          ]"
          @click="copyField(card.value, card.label, card.key)"
        >
          <div class="flex items-center justify-between gap-1">
            <span class="text-[10.5px] text-slate-400 truncate">{{ card.label }}</span>
            <IconRenderer
              :name="isCopied(card.key) ? 'Check' : 'Copy'"
              :size="12"
              :class-name="isCopied(card.key) ? 'text-emerald-400 shrink-0' : 'text-slate-400 shrink-0'"
            />
          </div>
          <div class="font-mono text-[13.5px] font-bold tracking-tight my-1 truncate select-text">
            {{ card.value }}
          </div>
          <div class="text-[10px] text-slate-400 truncate">
            {{ card.sub }}
          </div>
        </div>
      </div>

      <!-- Bottom Split Tools: [Left: VLSM Subnet Splitter] + [Right: IP in CIDR Checker] -->
      <div v-if="cidrAnalysis" class="grid grid-cols-12 gap-3">
        <!-- Left 7 Cols: VLSM Subnet Splitter -->
        <div
          class="col-span-7 p-3.5 rounded-2xl border space-y-2.5"
          :class="
            store.isDark
              ? 'bg-[#121929]/80 border-slate-800/90'
              : 'bg-white border-slate-200/85'
          "
        >
          <div class="flex items-center justify-between gap-2">
            <div class="text-xs font-bold">
              VLSM 子网进一步划分 ({{ cidrAnalysis.standardCidr }} 拆分)
            </div>
            <div v-if="splitOptions.length > 0" class="flex items-center gap-1">
              <button
                v-for="opt in splitOptions"
                :key="opt.targetPf"
                type="button"
                class="px-2 py-0.5 rounded-md border font-mono text-[10.5px] cursor-pointer transition-colors"
                :class="
                  splitTargetPrefix === opt.targetPf
                    ? store.isDark
                      ? 'bg-indigo-500/25 border-indigo-500 text-indigo-300 font-bold'
                      : 'bg-indigo-600 border-indigo-600 text-white font-bold'
                    : store.isDark
                      ? 'bg-slate-900 border-slate-800 text-slate-400'
                      : 'bg-slate-50 border-slate-200 text-slate-600'
                "
                @click="splitTargetPrefix = opt.targetPf"
              >
                /{{ opt.targetPf }} ({{ opt.subnetCount }}个子网)
              </button>
            </div>
          </div>

          <div
            v-if="splitSubnetsList.length > 0"
            class="max-h-[155px] overflow-y-auto space-y-1 pr-1"
          >
            <div
              v-for="sub in splitSubnetsList"
              :key="sub.cidr"
              class="px-2.5 py-1.5 rounded-lg border flex items-center justify-between text-[11px] font-mono cursor-pointer transition-colors"
              :class="[
                isCopied(`sub-${sub.cidr}`)
                  ? 'border-emerald-500/60 bg-emerald-500/10'
                  : store.isDark
                    ? 'bg-slate-950/60 border-slate-800/80 hover:border-slate-700'
                    : 'bg-slate-50 border-slate-200/75 hover:border-slate-300'
              ]"
              @click="copyField(`${sub.cidr} (${sub.range})`, `子网 #${sub.index}`, `sub-${sub.cidr}`)"
            >
              <div class="flex items-center gap-2 min-w-0">
                <span class="text-slate-400">#{{ sub.index }}</span>
                <span class="font-bold text-indigo-400">{{ sub.cidr }}</span>
                <span class="text-slate-400 truncate">可用: {{ sub.range }}</span>
              </div>
              <IconRenderer
                :name="isCopied(`sub-${sub.cidr}`) ? 'Check' : 'Copy'"
                :size="11"
                :class-name="isCopied(`sub-${sub.cidr}`) ? 'text-emerald-400 shrink-0' : 'text-slate-400 shrink-0'"
              />
            </div>
          </div>
          <div v-else class="text-xs text-slate-400 py-4 text-center">
            当前 /{{ cidrAnalysis.prefix }} 已是最小主机单元，无法继续拆分
          </div>
        </div>

        <!-- Right 5 Cols: IP in CIDR Inclusion Checker -->
        <div
          class="col-span-5 p-3.5 rounded-2xl border flex flex-col justify-between space-y-2.5"
          :class="
            store.isDark
              ? 'bg-[#121929]/80 border-slate-800/90'
              : 'bg-white border-slate-200/85'
          "
        >
          <div class="space-y-2">
            <div class="text-xs font-bold">IP 归属网段校验 (IP in CIDR)</div>
            <div class="text-[11px] text-slate-400">
              输入任意 IPv4 地址，判断其是否落在当前网段
              <code class="font-mono text-indigo-400">{{ cidrAnalysis.standardCidr }}</code>
              范围内：
            </div>
            <input
              v-model="testIpInCidr"
              type="text"
              :placeholder="`例如 ${cidrAnalysis.firstHostIp}`"
              aria-label="输入待校验的 IPv4 地址"
              class="w-full h-[32px] px-3 rounded-xl border font-mono text-xs outline-none"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-700 text-slate-100 focus:border-indigo-500'
                  : 'bg-slate-50 border-slate-200 text-slate-800 focus:border-indigo-500'
              "
            />
          </div>

          <div
            v-if="ipInCidrCheckResult"
            class="p-2.5 rounded-xl border text-xs font-medium"
            :class="
              !ipInCidrCheckResult.valid
                ? 'bg-amber-500/10 border-amber-500/30 text-amber-400'
                : ipInCidrCheckResult.inside
                  ? 'bg-emerald-500/15 border-emerald-500/35 text-emerald-400'
                  : 'bg-rose-500/15 border-rose-500/35 text-rose-400'
            "
          >
            {{ ipInCidrCheckResult.message }}
          </div>
          <div v-else class="text-[11px] text-slate-400">
            常用于排查防火墙白名单、VPC 路由表与 K8s Pod CIDR 连通性。
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
