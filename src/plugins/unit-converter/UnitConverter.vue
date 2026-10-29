<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useLauncherStore } from '@/stores/launcher'
import IconRenderer from '@/components/IconRenderer.vue'

const store = useLauncherStore()

type StudioMode = 'unit' | 'format'
const activeMode = ref<StudioMode>('unit')

// ============================================================================
// 1. In-Place Copy Feedback State (Consistent with PasswordVault & IpStudio)
// ============================================================================
const lastCopiedKey = ref<string | null>(null)
let copyFeedbackTimer: ReturnType<typeof setTimeout> | null = null

async function copyField(val: string | number | undefined | null, label: string, keyId: string) {
  const clean = String(val ?? '').trim()
  if (!clean || clean === '--') return
  try {
    await navigator.clipboard.writeText(clean)
    lastCopiedKey.value = keyId
    if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer)
    copyFeedbackTimer = setTimeout(() => {
      if (lastCopiedKey.value === keyId) {
        lastCopiedKey.value = null
      }
    }, 1350)
    store.showToast(`已复制${label}：${clean.length > 36 ? clean.slice(0, 36) + '...' : clean}`)
  } catch {
    // ignore
  }
}

function isCopied(keyId: string): boolean {
  return lastCopiedKey.value === keyId
}

// ============================================================================
// 2. TAB 1: 度量衡单位换算引擎 (10 Core Categories, One-to-Many Live Matrix)
// ============================================================================
type UnitCategoryId =
  | 'data'
  | 'bandwidth'
  | 'length'
  | 'area'
  | 'volume'
  | 'mass'
  | 'temperature'
  | 'time'
  | 'speed'
  | 'energy'
  | 'pressure'
  | 'base_rmb'

// Default to 1024 binary standard for data storage (1 GB = 1024 MB = 1,073,741,824 B)
const dataStorageRadix = ref<1024 | 1000>(1024)

interface UnitDef {
  id: string
  name: string
  symbol: string
  group: string
  /** Factor relative to category base unit (value_in_base = val * factor) */
  factor: number
}

interface CategoryDef {
  id: UnitCategoryId
  label: string
  shortLabel: string
  defaultUnit: string
  defaultVal: string
  presets: { label: string; val: string; unit: string }[]
  units: UnitDef[]
}

const UNIT_CATEGORIES: CategoryDef[] = [
  {
    id: 'data',
    label: '数据存储容量',
    shortLabel: '数据存储',
    defaultUnit: 'GB',
    defaultVal: '1',
    presets: [
      { label: '1 GB', val: '1', unit: 'GB' },
      { label: '1 TB 硬盘', val: '1', unit: 'TB' },
      { label: '512 GB 硬盘', val: '512', unit: 'GB' },
      { label: '256 GB', val: '256', unit: 'GB' },
      { label: '16 GB 内存', val: '16', unit: 'GB' },
      { label: '64 MB 配置', val: '64', unit: 'MB' }
    ],
    // Base unit: Byte (B); factors for KB..EB are dynamically computed using dataStorageRadix (1024 or 1000)
    units: [
      { id: 'bit', name: '比特 / 位 (bit)', symbol: 'b', group: '1 Byte = 8 bits', factor: 0.125 },
      { id: 'B', name: '字节 (Byte)', symbol: 'B', group: '基础字节单元', factor: 1 },
      { id: 'KB', name: '千字节 (Kilobyte)', symbol: 'KB', group: '1024 B', factor: 1024 },
      { id: 'MB', name: '兆字节 (Megabyte)', symbol: 'MB', group: '1024 KB', factor: Math.pow(1024, 2) },
      { id: 'GB', name: '吉字节 (Gigabyte)', symbol: 'GB', group: '1024 MB', factor: Math.pow(1024, 3) },
      { id: 'TB', name: '太字节 (Terabyte)', symbol: 'TB', group: '1024 GB', factor: Math.pow(1024, 4) },
      { id: 'PB', name: '拍字节 (Petabyte)', symbol: 'PB', group: '1024 TB', factor: Math.pow(1024, 5) },
      { id: 'EB', name: '艾字节 (Exabyte)', symbol: 'EB', group: '1024 PB', factor: Math.pow(1024, 6) }
    ]
  },
  {
    id: 'bandwidth',
    label: '网络宽带与下载速率',
    shortLabel: '网络带宽',
    defaultUnit: 'Mbps',
    defaultVal: '1000',
    presets: [
      { label: '100 Mbps 百兆宽带', val: '100', unit: 'Mbps' },
      { label: '300 Mbps 家用宽带', val: '300', unit: 'Mbps' },
      { label: '500 Mbps 宽带', val: '500', unit: 'Mbps' },
      { label: '1000 Mbps 千兆宽带', val: '1000', unit: 'Mbps' },
      { label: '2.5 Gbps 网卡', val: '2.5', unit: 'Gbps' },
      { label: '10 Gbps 万兆网卡', val: '10', unit: 'Gbps' }
    ],
    // Base unit: bit per second (bps)
    units: [
      { id: 'Mbps', name: '兆比特/秒 (宽带兆数)', symbol: 'Mbps', group: '运营商宽带标称 (bit/s)', factor: 1e6 },
      { id: 'Gbps', name: '吉比特/秒 (千兆/万兆)', symbol: 'Gbps', group: '运营商宽带标称 (bit/s)', factor: 1e9 },
      { id: 'Kbps', name: '千比特/秒', symbol: 'Kbps', group: '运营商宽带标称 (bit/s)', factor: 1e3 },
      { id: 'bps', name: '比特/秒', symbol: 'bps', group: '运营商宽带标称 (bit/s)', factor: 1 },
      { id: 'MBs', name: '兆字节/秒 (实际下载速度)', symbol: 'MB/s', group: '软件实际下载速度 (Byte/s)', factor: 8e6 },
      { id: 'GBs', name: '吉字节/秒', symbol: 'GB/s', group: '软件实际下载速度 (Byte/s)', factor: 8e9 },
      { id: 'KBs', name: '千字节/秒', symbol: 'KB/s', group: '软件实际下载速度 (Byte/s)', factor: 8e3 },
      { id: 'Bs', name: '字节/秒', symbol: 'B/s', group: '软件实际下载速度 (Byte/s)', factor: 8 }
    ]
  },
  {
    id: 'length',
    label: '长度与距离',
    shortLabel: '长度',
    defaultUnit: 'm',
    defaultVal: '1',
    presets: [
      { label: '1 米', val: '1', unit: 'm' },
      { label: '100 厘米', val: '100', unit: 'cm' },
      { label: '27 英寸显示器', val: '27', unit: 'in' },
      { label: '6 英尺身高', val: '6', unit: 'ft' },
      { label: '42.195 km 全马', val: '42.195', unit: 'km' }
    ],
    // Base unit: Meter (m)
    units: [
      { id: 'km', name: '千米 / 公里', symbol: 'km', group: '国际公制', factor: 1000 },
      { id: 'm', name: '米', symbol: 'm', group: '国际公制', factor: 1 },
      { id: 'dm', name: '分米', symbol: 'dm', group: '国际公制', factor: 0.1 },
      { id: 'cm', name: '厘米', symbol: 'cm', group: '国际公制', factor: 0.01 },
      { id: 'mm', name: '毫米', symbol: 'mm', group: '国际公制', factor: 0.001 },
      { id: 'um', name: '微米', symbol: 'μm', group: '国际公制', factor: 1e-6 },
      { id: 'nm', name: '纳米', symbol: 'nm', group: '国际公制', factor: 1e-9 },
      { id: 'li', name: '市里 (500m)', symbol: '里', group: '中国市制', factor: 500 },
      { id: 'zhang', name: '丈', symbol: '丈', group: '中国市制', factor: 10 / 3 },
      { id: 'chi', name: '尺', symbol: '尺', group: '中国市制', factor: 1 / 3 },
      { id: 'cun', name: '寸', symbol: '寸', group: '中国市制', factor: 1 / 30 },
      { id: 'mi', name: '英里 (Mile)', symbol: 'mi', group: '英制与航海', factor: 1609.344 },
      { id: 'yd', name: '码 (Yard)', symbol: 'yd', group: '英制与航海', factor: 0.9144 },
      { id: 'ft', name: '英尺 (Foot)', symbol: 'ft', group: '英制与航海', factor: 0.3048 },
      { id: 'in', name: '英寸 (Inch)', symbol: 'in', group: '英制与航海', factor: 0.0254 },
      { id: 'nmi', name: '海里 (Nautical)', symbol: 'nmi', group: '英制与航海', factor: 1852 }
    ]
  },
  {
    id: 'area',
    label: '面积与房产土地',
    shortLabel: '面积',
    defaultUnit: 'm2',
    defaultVal: '100',
    presets: [
      { label: '100 m² 住宅', val: '100', unit: 'm2' },
      { label: '1000 平方尺(香港)', val: '1000', unit: 'sqft' },
      { label: '30 坪(台湾/日本)', val: '30', unit: 'ping' },
      { label: '1 亩地', val: '1', unit: 'mu' },
      { label: '1 公顷', val: '1', unit: 'ha' }
    ],
    // Base unit: m²
    units: [
      { id: 'km2', name: '平方千米', symbol: 'km²', group: '国际公制', factor: 1e6 },
      { id: 'ha', name: '公顷 (Hectare)', symbol: 'ha', group: '国际公制', factor: 10000 },
      { id: 'm2', name: '平方米', symbol: 'm²', group: '国际公制', factor: 1 },
      { id: 'dm2', name: '平方分米', symbol: 'dm²', group: '国际公制', factor: 0.01 },
      { id: 'cm2', name: '平方厘米', symbol: 'cm²', group: '国际公制', factor: 1e-4 },
      { id: 'mm2', name: '平方毫米', symbol: 'mm²', group: '国际公制', factor: 1e-6 },
      { id: 'mu', name: '亩 (中国市制)', symbol: '亩', group: '房产与土地', factor: 2000 / 3 },
      { id: 'fen', name: '分 (0.1亩)', symbol: '分', group: '房产与土地', factor: 200 / 3 },
      {
        id: 'sqft',
        name: '平方英尺 / 港尺',
        symbol: 'ft²',
        group: '房产与土地',
        factor: 0.09290304
      },
      {
        id: 'ping',
        name: '坪 (台湾/日本房产)',
        symbol: '坪',
        group: '房产与土地',
        factor: 400 / 121
      },
      { id: 'ac', name: '英亩 (Acre)', symbol: 'ac', group: '英制面积', factor: 4046.8564224 },
      { id: 'yd2', name: '平方码', symbol: 'yd²', group: '英制面积', factor: 0.83612736 }
    ]
  },
  {
    id: 'volume',
    label: '体积与餐饮容积',
    shortLabel: '体积',
    defaultUnit: 'L',
    defaultVal: '1',
    presets: [
      { label: '1 升 (L)', val: '1', unit: 'L' },
      { label: '500 毫升', val: '500', unit: 'mL' },
      { label: '16 oz 咖啡杯', val: '16', unit: 'floz_us' },
      { label: '1 美制加仑', val: '1', unit: 'gal_us' },
      { label: '1 立方米', val: '1', unit: 'm3' }
    ],
    // Base unit: Liter (L)
    units: [
      { id: 'm3', name: '立方米', symbol: 'm³', group: '国际公制', factor: 1000 },
      { id: 'L', name: '升 / 立方分米', symbol: 'L', group: '国际公制', factor: 1 },
      { id: 'dL', name: '分升 (100mL)', symbol: 'dL', group: '国际公制', factor: 0.1 },
      { id: 'mL', name: '毫升 / 立方厘米 (cc)', symbol: 'mL', group: '国际公制', factor: 0.001 },
      { id: 'mm3', name: '立方毫米', symbol: 'mm³', group: '国际公制', factor: 1e-6 },
      { id: 'gal_us', name: '美制加仑', symbol: 'US gal', group: '英美与餐饮', factor: 3.785411784 },
      { id: 'gal_uk', name: '英制加仑', symbol: 'UK gal', group: '英美与餐饮', factor: 4.54609 },
      { id: 'pt_us', name: '美制品脱 (Pint)', symbol: 'US pt', group: '英美与餐饮', factor: 0.473176473 },
      {
        id: 'floz_us',
        name: '美制液量盎司',
        symbol: 'fl oz',
        group: '英美与餐饮',
        factor: 0.0295735296
      },
      { id: 'cup', name: '标准量杯 (240mL)', symbol: 'cup', group: '烘焙量具', factor: 0.24 },
      { id: 'tbsp', name: '汤匙 (15mL)', symbol: 'tbsp', group: '烘焙量具', factor: 0.015 },
      { id: 'tsp', name: '茶匙 (5mL)', symbol: 'tsp', group: '烘焙量具', factor: 0.005 }
    ]
  },
  {
    id: 'mass',
    label: '质量与重量',
    shortLabel: '质量',
    defaultUnit: 'kg',
    defaultVal: '1',
    presets: [
      { label: '1 千克 (公斤)', val: '1', unit: 'kg' },
      { label: '1 斤 (500g)', val: '1', unit: 'jin' },
      { label: '100 磅 (lb)', val: '100', unit: 'lb' },
      { label: '1 金衡盎司黄金', val: '1', unit: 'ozt' },
      { label: '1 克拉钻石', val: '1', unit: 'ct' }
    ],
    // Base unit: Gram (g)
    units: [
      { id: 't', name: '吨 (Metric Ton)', symbol: 't', group: '国际公制', factor: 1e6 },
      { id: 'kg', name: '千克 / 公斤', symbol: 'kg', group: '国际公制', factor: 1000 },
      { id: 'g', name: '克', symbol: 'g', group: '国际公制', factor: 1 },
      { id: 'mg', name: '毫克', symbol: 'mg', group: '国际公制', factor: 0.001 },
      { id: 'ug', name: '微克', symbol: 'μg', group: '国际公制', factor: 1e-6 },
      { id: 'jin', name: '市斤 (500g)', symbol: '斤', group: '中国市制', factor: 500 },
      { id: 'liang', name: '两 (50g)', symbol: '两', group: '中国市制', factor: 50 },
      { id: 'qian', name: '钱 (5g)', symbol: '钱', group: '中国市制', factor: 5 },
      { id: 'lb', name: '磅 (Pound)', symbol: 'lb', group: '英制与贵金属', factor: 453.59237 },
      { id: 'oz', name: '常衡盎司 (海淘/日常)', symbol: 'oz', group: '英制与贵金属', factor: 28.349523125 },
      {
        id: 'ozt',
        name: '金衡盎司 (黄金/白银)',
        symbol: 'oz t',
        group: '英制与贵金属',
        factor: 31.1034768
      },
      { id: 'ct', name: '克拉 (钻石珠宝)', symbol: 'ct', group: '英制与贵金属', factor: 0.2 }
    ]
  },
  {
    id: 'temperature',
    label: '温度标度',
    shortLabel: '温度',
    defaultUnit: 'C',
    defaultVal: '25',
    presets: [
      { label: '0°C 水冰点', val: '0', unit: 'C' },
      { label: '25°C 舒适室温', val: '25', unit: 'C' },
      { label: '37°C 人体常温', val: '37', unit: 'C' },
      { label: '100°C 水沸点', val: '100', unit: 'C' },
      { label: '98.6°F 华氏体温', val: '98.6', unit: 'F' },
      { label: '0 K 绝对零度', val: '0', unit: 'K' }
    ],
    units: [
      { id: 'C', name: '摄氏度 (Celsius)', symbol: '°C', group: '温度标度', factor: 1 },
      { id: 'F', name: '华氏度 (Fahrenheit)', symbol: '°F', group: '温度标度', factor: 1 },
      { id: 'K', name: '开尔文 (Kelvin 热力学)', symbol: 'K', group: '温度标度', factor: 1 },
      { id: 'R', name: '兰氏度 (Rankine)', symbol: '°R', group: '温度标度', factor: 1 },
      { id: 'Re', name: '列氏度 (Réaumur)', symbol: '°Ré', group: '温度标度', factor: 1 }
    ]
  },
  {
    id: 'time',
    label: '时间与时长',
    shortLabel: '时间',
    defaultUnit: 'h',
    defaultVal: '24',
    presets: [
      { label: '1 天 (24h)', val: '1', unit: 'd' },
      { label: '1 周 (7天)', val: '1', unit: 'wk' },
      { label: '86400 秒', val: '86400', unit: 's' },
      { label: '90061 秒 (复合测试)', val: '90061', unit: 's' },
      { label: '1 年 (365天)', val: '1', unit: 'yr' }
    ],
    // Base unit: Second (s)
    units: [
      { id: 'yr', name: '年 (按 365 天)', symbol: 'yr', group: '宏观日期', factor: 31536000 },
      { id: 'mo', name: '月 (按 30 天)', symbol: 'mo', group: '宏观日期', factor: 2592000 },
      { id: 'wk', name: '周 / 星期', symbol: 'wk', group: '宏观日期', factor: 604800 },
      { id: 'd', name: '天 / 日', symbol: 'd', group: '日常时间', factor: 86400 },
      { id: 'h', name: '小时 (Hour)', symbol: 'h', group: '日常时间', factor: 3600 },
      { id: 'min', name: '分钟 (Minute)', symbol: 'min', group: '日常时间', factor: 60 },
      { id: 's', name: '秒 (Second)', symbol: 's', group: '日常时间', factor: 1 },
      { id: 'ms', name: '毫秒 (Millisecond)', symbol: 'ms', group: '计算机微观时间', factor: 1e-3 },
      { id: 'us', name: '微秒 (Microsecond)', symbol: 'μs', group: '计算机微观时间', factor: 1e-6 },
      { id: 'ns', name: '纳秒 (Nanosecond)', symbol: 'ns', group: '计算机微观时间', factor: 1e-9 }
    ]
  },
  {
    id: 'speed',
    label: '速度与运动配速',
    shortLabel: '速度',
    defaultUnit: 'kmh',
    defaultVal: '100',
    presets: [
      { label: '12 km/h 跑步(5分配)', val: '12', unit: 'kmh' },
      { label: '120 km/h 高速限速', val: '120', unit: 'kmh' },
      { label: '350 km/h 高铁复兴号', val: '350', unit: 'kmh' },
      { label: '1 马赫 (音速)', val: '1', unit: 'mach' }
    ],
    // Base unit: m/s
    units: [
      { id: 'kmh', name: '千米/小时 (公里/时)', symbol: 'km/h', group: '常用速度', factor: 1 / 3.6 },
      { id: 'ms', name: '米/秒', symbol: 'm/s', group: '常用速度', factor: 1 },
      { id: 'mph', name: '英里/小时', symbol: 'mph', group: '常用速度', factor: 0.44704 },
      { id: 'kn', name: '节 (海里/小时)', symbol: 'kn', group: '航海与航空', factor: 1852 / 3600 },
      { id: 'mach', name: '马赫 (标准音速)', symbol: 'Mach', group: '航海与航空', factor: 340.3 },
      { id: 'fts', name: '英尺/秒', symbol: 'ft/s', group: '工程速度', factor: 0.3048 },
      { id: 'kmmin', name: '千米/分钟', symbol: 'km/min', group: '工程速度', factor: 1000 / 60 },
      { id: 'c', name: '真空光速 (c)', symbol: 'c', group: '物理常数', factor: 299792458 }
    ]
  },
  {
    id: 'energy',
    label: '热量与能量 (饮食/电力)',
    shortLabel: '热量',
    defaultUnit: 'kJ',
    defaultVal: '1000',
    presets: [
      { label: '1000 kJ 食品营养表', val: '1000', unit: 'kJ' },
      { label: '500 kcal 健身大卡', val: '500', unit: 'kcal' },
      { label: '1 度电 (kWh)', val: '1', unit: 'kWh' }
    ],
    // Base unit: Joule (J)
    units: [
      { id: 'kcal', name: '千卡 / 大卡 (健身热量)', symbol: 'kcal', group: '饮食与卡路里', factor: 4184 },
      { id: 'kJ', name: '千焦 (包装食品营养表)', symbol: 'kJ', group: '饮食与卡路里', factor: 1000 },
      { id: 'cal', name: '卡路里 (小卡)', symbol: 'cal', group: '饮食与卡路里', factor: 4.184 },
      { id: 'J', name: '焦耳 (国际单位)', symbol: 'J', group: '物理与电力', factor: 1 },
      { id: 'kWh', name: '千瓦时 / 度电', symbol: 'kWh', group: '物理与电力', factor: 3.6e6 },
      { id: 'Wh', name: '瓦时 (电池容量)', symbol: 'Wh', group: '物理与电力', factor: 3600 },
      { id: 'BTU', name: '英热单位 (空调制冷)', symbol: 'BTU', group: '物理与电力', factor: 1055.05585 },
      { id: 'MJ', name: '兆焦耳', symbol: 'MJ', group: '物理与电力', factor: 1e6 }
    ]
  },
  {
    id: 'pressure',
    label: '压力与汽车胎压',
    shortLabel: '压力',
    defaultUnit: 'bar',
    defaultVal: '2.5',
    presets: [
      { label: '2.5 bar 标准胎压', val: '2.5', unit: 'bar' },
      { label: '36 psi 美标胎压', val: '36', unit: 'psi' },
      { label: '1 标准大气压', val: '1', unit: 'atm' },
      { label: '120 mmHg 血压', val: '120', unit: 'mmHg' }
    ],
    // Base unit: Pascal (Pa)
    units: [
      { id: 'bar', name: '巴 (汽车胎压常用)', symbol: 'bar', group: '胎压与工程', factor: 1e5 },
      { id: 'psi', name: '磅力/平方英寸 (美标胎压)', symbol: 'psi', group: '胎压与工程', factor: 6894.75729 },
      { id: 'kPa', name: '千帕 (国标胎压)', symbol: 'kPa', group: '胎压与工程', factor: 1000 },
      { id: 'MPa', name: '兆帕 (液压/工程)', symbol: 'MPa', group: '胎压与工程', factor: 1e6 },
      { id: 'Pa', name: '帕斯卡', symbol: 'Pa', group: '气象与医学', factor: 1 },
      { id: 'hPa', name: '百帕 / 毫巴 (气象气压)', symbol: 'hPa', group: '气象与医学', factor: 100 },
      { id: 'atm', name: '标准大气压', symbol: 'atm', group: '气象与医学', factor: 101325 },
      { id: 'mmHg', name: '毫米汞柱 (血压计)', symbol: 'mmHg', group: '气象与医学', factor: 133.322387 }
    ]
  },
  {
    id: 'base_rmb',
    label: '进制转换与财务大写',
    shortLabel: '进制/大写',
    defaultUnit: 'DEC',
    defaultVal: '12345.67',
    presets: [
      { label: '12345.67 报销金额', val: '12345.67', unit: 'DEC' },
      { label: '255 (0xFF)', val: '255', unit: 'DEC' },
      { label: '1024 (2^10)', val: '1024', unit: 'DEC' },
      { label: '65535 (0xFFFF)', val: '65535', unit: 'DEC' }
    ],
    units: []
  }
]

const activeCategoryId = ref<UnitCategoryId>('data')
const unitInputVal = ref<string>('1')
const unitSourceId = ref<string>('GB')

const activeCategory = computed(
  () => UNIT_CATEGORIES.find((c) => c.id === activeCategoryId.value) || UNIT_CATEGORIES[0]
)

function selectUnitCategory(catId: UnitCategoryId) {
  activeCategoryId.value = catId
  const cat = UNIT_CATEGORIES.find((c) => c.id === catId)
  if (cat) {
    unitSourceId.value = cat.defaultUnit
    unitInputVal.value = cat.defaultVal
  }
}

function applyUnitPreset(val: string, unit: string) {
  unitInputVal.value = val
  unitSourceId.value = unit
}

function formatSmartNumber(n: number): string {
  if (!Number.isFinite(n)) return '--'
  if (n === 0) return '0'
  const abs = Math.abs(n)
  if (abs >= 1e12 || (abs > 0 && abs < 1e-6)) {
    return n.toExponential(4).replace(/\.?0+e/, 'e')
  }
  // Check if nearly integer
  if (Math.abs(n - Math.round(n)) < 1e-9) {
    return Math.round(n).toLocaleString('en-US')
  }
  if (abs >= 100) {
    return Number(n.toFixed(4)).toLocaleString('en-US', { maximumFractionDigits: 4 })
  }
  return Number(n.toPrecision(7)).toString()
}

// Convert temperature via Celsius intermediate
function convertTemperature(val: number, fromId: string, toId: string): number {
  let c = val
  if (fromId === 'F') c = ((val - 32) * 5) / 9
  else if (fromId === 'K') c = val - 273.15
  else if (fromId === 'R') c = ((val - 491.67) * 5) / 9
  else if (fromId === 'Re') c = val * 1.25

  if (toId === 'C') return c
  if (toId === 'F') return (c * 9) / 5 + 32
  if (toId === 'K') return c + 273.15
  if (toId === 'R') return ((c + 273.15) * 9) / 5
  if (toId === 'Re') return c * 0.8
  return c
}

function getDataUnitDynamicFactor(unitId: string, radix: 1024 | 1000): { factor: number; desc: string } {
  const r = radix
  switch (unitId) {
    case 'bit':
      return { factor: 0.125, desc: '1 Byte = 8 bits' }
    case 'B':
      return { factor: 1, desc: '基础字节单元' }
    case 'KB':
      return { factor: Math.pow(r, 1), desc: `1 KB = ${r} B` }
    case 'MB':
      return { factor: Math.pow(r, 2), desc: `1 MB = ${r} KB` }
    case 'GB':
      return { factor: Math.pow(r, 3), desc: `1 GB = ${r} MB` }
    case 'TB':
      return { factor: Math.pow(r, 4), desc: `1 TB = ${r} GB` }
    case 'PB':
      return { factor: Math.pow(r, 5), desc: `1 PB = ${r} TB` }
    case 'EB':
      return { factor: Math.pow(r, 6), desc: `1 EB = ${r} PB` }
    default:
      return { factor: 1, desc: '' }
  }
}

const computedUnitMatrix = computed(() => {
  const cat = activeCategory.value
  if (cat.id === 'base_rmb') return []
  const rawNum = Number(unitInputVal.value.replace(/,/g, '').trim())
  const isValid = unitInputVal.value.trim() !== '' && !Number.isNaN(rawNum)

  const srcUnit = cat.units.find((u) => u.id === unitSourceId.value) || cat.units[0]
  const srcFactor =
    cat.id === 'data'
      ? getDataUnitDynamicFactor(srcUnit.id, dataStorageRadix.value).factor
      : srcUnit.factor
  const baseValue = isValid ? rawNum * srcFactor : NaN

  return cat.units.map((u) => {
    const uDynamic =
      cat.id === 'data'
        ? getDataUnitDynamicFactor(u.id, dataStorageRadix.value)
        : { factor: u.factor, desc: u.group }

    if (!isValid) {
      return { ...u, group: uDynamic.desc, formatted: '--', raw: '' }
    }
    let targetVal = 0
    if (cat.id === 'temperature') {
      targetVal = convertTemperature(rawNum, srcUnit.id, u.id)
    } else {
      targetVal = baseValue / uDynamic.factor
    }
    return {
      ...u,
      group: uDynamic.desc,
      formatted: formatSmartNumber(targetVal),
      raw: Number(targetVal.toPrecision(10)).toString()
    }
  })
})

// Real-time Disk Capacity Shrinkage Comparison (Manufacturer 1000 vs OS 1024)
const diskCapacityShrinkInsight = computed(() => {
  if (activeCategory.value.id !== 'data') return null
  const rawNum = Number(unitInputVal.value.replace(/,/g, '').trim())
  if (!unitInputVal.value.trim() || Number.isNaN(rawNum) || rawNum <= 0) return null
  const powerMap: Record<string, number> = { KB: 1, MB: 2, GB: 3, TB: 4, PB: 5, EB: 6 }
  const p = powerMap[unitSourceId.value]
  if (!p) return null

  const vendorBytes = rawNum * Math.pow(1000, p)
  const osActualVal = vendorBytes / Math.pow(1024, p)
  const osFormatted = Number(osActualVal.toFixed(2)).toLocaleString('en-US')
  const ratioPct = ((osActualVal / rawNum) * 100).toFixed(2)
  return {
    vendorLabel: `${rawNum} ${unitSourceId.value}`,
    osValue: `${osFormatted} ${unitSourceId.value}`,
    osRaw: osActualVal.toFixed(2),
    text: `硬盘标称折算：厂商按 1000 进制标称 ${rawNum} ${unitSourceId.value} 时，在 Windows (1024 进制) 中实际显示为 ${osFormatted} ${unitSourceId.value}（约为标称值的 ${ratioPct}%）`
  }
})

// Contextual Insight Banner for Bandwidth / Time / Speed
const categoryInsightBanner = computed(() => {
  const cat = activeCategory.value
  const rawNum = Number(unitInputVal.value.replace(/,/g, '').trim())
  if (!unitInputVal.value.trim() || Number.isNaN(rawNum) || rawNum <= 0) return null

  const srcUnit = cat.units.find((u) => u.id === unitSourceId.value) || cat.units[0]

  if (cat.id === 'bandwidth') {
    const bps = rawNum * srcUnit.factor
    const bytesPerSec = bps / 8
    if (bytesPerSec > 0) {
      const fmtSec = (sec: number) => {
        if (sec < 1) return `${(sec * 1000).toFixed(0)}毫秒`
        if (sec < 60) return `${sec.toFixed(1)}秒`
        if (sec < 3600) return `${Math.floor(sec / 60)}分${Math.round(sec % 60)}秒`
        return `${(sec / 3600).toFixed(1)}小时`
      }
      const sec1G = Math.pow(1024, 3) / bytesPerSec
      const sec10G = (10 * Math.pow(1024, 3)) / bytesPerSec
      return `理论下载耗时：在该速率下，下载 1 GB 文件约需 ${fmtSec(sec1G)} · 下载 10 GB 文件约需 ${fmtSec(sec10G)}`
    }
  }

  if (cat.id === 'time') {
    const totalSec = Math.floor(rawNum * srcUnit.factor)
    if (totalSec >= 60) {
      const d = Math.floor(totalSec / 86400)
      const h = Math.floor((totalSec % 86400) / 3600)
      const m = Math.floor((totalSec % 3600) / 60)
      const s = totalSec % 60
      const parts: string[] = []
      if (d > 0) parts.push(`${d}天`)
      if (h > 0) parts.push(`${h}小时`)
      if (m > 0) parts.push(`${m}分`)
      if (s > 0) parts.push(`${s}秒`)
      return `复合时长拆解：${parts.join(' ') || '0秒'}`
    }
  }

  if (cat.id === 'speed') {
    const ms = rawNum * srcUnit.factor
    const kmh = ms * 3.6
    if (kmh > 0.5 && kmh <= 45) {
      const secPerKm = 3600 / kmh
      const paceMin = Math.floor(secPerKm / 60)
      const paceSec = Math.round(secPerKm % 60)
      const fullMaraSec = secPerKm * 42.195
      const mh = Math.floor(fullMaraSec / 3600)
      const mm = Math.floor((fullMaraSec % 3600) / 60)
      return `跑步配速换算：${paceMin}'${String(paceSec).padStart(2, '0')}" /公里 · 全马 (42.195km) 预计完赛时间：${mh}小时${mm}分`
    }
  }

  return null
})

// ============================================================================
// 2B. Number Base & Chinese Financial RMB Uppercase Converter (`base_rmb`)
// ============================================================================
const baseInputRadix = ref<number>(10)
const baseInputStr = ref<string>('12345.67')

function numberToChineseRMB(amount: number): string {
  if (!Number.isFinite(amount) || amount < 0) return '仅支持非负金额'
  if (amount > 999999999999.99) return '超出千亿级金额上限'
  const cnNums = ['零', '壹', '贰', '叁', '肆', '伍', '陆', '柒', '捌', '玖']
  const cnIntRadice = ['', '拾', '佰', '仟']
  const cnIntUnits = ['', '万', '亿', '兆']
  const cnDecUnits = ['角', '分']

  const rounded = Math.round(amount * 100)
  if (rounded === 0) return '零元整'

  const integerNum = Math.floor(rounded / 100)
  const decimalNum = rounded % 100

  let chineseStr = ''
  if (integerNum > 0) {
    let zeroCount = 0
    const intStr = integerNum.toString()
    const IntLen = intStr.length
    for (let i = 0; i < IntLen; i++) {
      const n = intStr.substr(i, 1)
      const p = IntLen - i - 1
      const q = Math.floor(p / 4)
      const m = p % 4
      if (n === '0') {
        zeroCount++
      } else {
        if (zeroCount > 0) {
          chineseStr += cnNums[0]
        }
        zeroCount = 0
        chineseStr += cnNums[parseInt(n, 10)] + cnIntRadice[m]
      }
      if (m === 0 && zeroCount < 4) {
        chineseStr += cnIntUnits[q]
      }
    }
    chineseStr += '元'
  }

  if (decimalNum === 0) {
    chineseStr += '整'
  } else {
    const jiao = Math.floor(decimalNum / 10)
    const fen = decimalNum % 10
    if (jiao > 0) {
      chineseStr += cnNums[jiao] + cnDecUnits[0]
    } else if (integerNum > 0 && fen > 0) {
      chineseStr += cnNums[0]
    }
    if (fen > 0) {
      chineseStr += cnNums[fen] + cnDecUnits[1]
    }
  }
  return chineseStr
}

const baseAndRmbResults = computed(() => {
  const clean = baseInputStr.value.trim().replace(/^0x/i, '').replace(/^0b/i, '')
  if (!clean) return null
  let numVal = NaN
  if (baseInputRadix.value === 10) {
    numVal = Number(baseInputStr.value.replace(/,/g, '').trim())
  } else {
    numVal = parseInt(clean, baseInputRadix.value)
  }
  if (Number.isNaN(numVal)) return null

  const intPart = Math.trunc(numVal)
  return {
    rmbUpper: numberToChineseRMB(Math.abs(numVal)),
    formattedCurrency: `¥ ${numVal.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`,
    dec: intPart.toString(10),
    hex: `0x${intPart.toString(16).toUpperCase()}`,
    hexRaw: intPart.toString(16).toUpperCase(),
    bin: intPart.toString(2).replace(/\B(?=(\d{4})+(?!\d))/g, ' '),
    oct: `0o${intPart.toString(8)}`,
    base36: intPart.toString(36).toUpperCase()
  }
})

// ============================================================================
// 3. TAB 2: 编码与格式转换工作台 (Timestamp / Color / Encoding & Hash / Query & Table)
// ============================================================================
type FormatToolId = 'timestamp' | 'color' | 'encoding' | 'data_format'
const activeFormatTool = ref<FormatToolId>('timestamp')

// --- 3.1 Timestamp & Multi-Timezone Converter ---
const liveNowMs = ref<number>(Date.now())
let clockTimer: ReturnType<typeof setInterval> | null = null
const tsInputStr = ref<string>(String(Math.floor(Date.now() / 1000)))

function setTsInputToNow(asMs = false) {
  const now = Date.now()
  tsInputStr.value = asMs ? String(now) : String(Math.floor(now / 1000))
}

const parsedTimestampCards = computed(() => {
  const raw = tsInputStr.value.trim()
  if (!raw) return null
  let dateObj: Date
  if (/^\d{9,10}$/.test(raw)) {
    dateObj = new Date(Number(raw) * 1000)
  } else if (/^\d{12,14}$/.test(raw)) {
    dateObj = new Date(Number(raw))
  } else {
    dateObj = new Date(raw.replace(/-/g, '/'))
    if (Number.isNaN(dateObj.getTime())) {
      dateObj = new Date(raw)
    }
  }
  if (Number.isNaN(dateObj.getTime())) return null

  const ms = dateObj.getTime()
  const sec = Math.floor(ms / 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  const localStr = `${dateObj.getFullYear()}-${pad(dateObj.getMonth() + 1)}-${pad(dateObj.getDate())} ${pad(dateObj.getHours())}:${pad(dateObj.getMinutes())}:${pad(dateObj.getSeconds())}`

  const startOfDay = new Date(dateObj.getFullYear(), dateObj.getMonth(), dateObj.getDate(), 0, 0, 0)
  const endOfDay = new Date(
    dateObj.getFullYear(),
    dateObj.getMonth(),
    dateObj.getDate(),
    23,
    59,
    59
  )

  const diffSec = Math.round((ms - liveNowMs.value) / 1000)
  let relativeStr = '刚刚'
  const absSec = Math.abs(diffSec)
  const suffix = diffSec >= 0 ? '后' : '前'
  if (absSec >= 86400) relativeStr = `${Math.floor(absSec / 86400)} 天${suffix}`
  else if (absSec >= 3600) relativeStr = `${Math.floor(absSec / 3600)} 小时${suffix}`
  else if (absSec >= 60) relativeStr = `${Math.floor(absSec / 60)} 分钟${suffix}`
  else if (absSec > 2) relativeStr = `${absSec} 秒${suffix}`

  const fmtTz = (tz: string) => {
    try {
      return new Intl.DateTimeFormat('zh-CN', {
        timeZone: tz,
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        hour12: false
      }).format(dateObj)
    } catch {
      return '--'
    }
  }

  return [
    { key: 'ts-sec', label: '秒级时间戳 (10位)', value: String(sec), sub: `相对当前: ${relativeStr}` },
    { key: 'ts-ms', label: '毫秒级时间戳 (13位)', value: String(ms), sub: 'JavaScript / Java 毫秒标准' },
    { key: 'ts-local', label: '本地标准时间', value: localStr, sub: 'YYYY-MM-DD HH:mm:ss' },
    { key: 'ts-iso', label: 'ISO 8601 (UTC)', value: dateObj.toISOString(), sub: '国际标准传输格式' },
    {
      key: 'ts-day-start',
      label: '当天零点 (00:00:00)',
      value: String(Math.floor(startOfDay.getTime() / 1000)),
      sub: `${dateObj.getFullYear()}-${pad(dateObj.getMonth() + 1)}-${pad(dateObj.getDate())} 00:00:00`
    },
    {
      key: 'ts-day-end',
      label: '当天末刻 (23:59:59)',
      value: String(Math.floor(endOfDay.getTime() / 1000)),
      sub: `${dateObj.getFullYear()}-${pad(dateObj.getMonth() + 1)}-${pad(dateObj.getDate())} 23:59:59`
    },
    {
      key: 'ts-us',
      label: '美东时间 (New York)',
      value: fmtTz('America/New_York'),
      sub: 'America/New_York'
    },
    {
      key: 'ts-tokyo',
      label: '东京时间 (Tokyo)',
      value: fmtTz('Asia/Tokyo'),
      sub: 'Asia/Tokyo (UTC+9)'
    }
  ]
})

// --- 3.2 Color Format Converter (HEX / RGB / HSL / CMYK) ---
const colorInputStr = ref<string>('#4F46E5')

function parseAnyColorToRgb(input: string): { r: number; g: number; b: number } | null {
  const s = input.trim()
  // 1. HEX (#RGB or #RRGGBB)
  const hexMatch = s.match(/^#?([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/)
  if (hexMatch) {
    let h = hexMatch[1]
    if (h.length === 3) {
      h = h
        .split('')
        .map((c) => c + c)
        .join('')
    }
    const num = parseInt(h, 16)
    return {
      r: (num >> 16) & 255,
      g: (num >> 8) & 255,
      b: num & 255
    }
  }
  // 2. rgb(r, g, b)
  const rgbMatch = s.match(/^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})/i)
  if (rgbMatch) {
    const r = Math.min(255, Number(rgbMatch[1]))
    const g = Math.min(255, Number(rgbMatch[2]))
    const b = Math.min(255, Number(rgbMatch[3]))
    return { r, g, b }
  }
  // 3. hsl(h, s%, l%)
  const hslMatch = s.match(/^hsla?\(\s*(\d{1,3})\s*,\s*(\d{1,3})%?\s*,\s*(\d{1,3})%?/i)
  if (hslMatch) {
    const h = (Number(hslMatch[1]) % 360) / 360
    const sat = Math.min(100, Number(hslMatch[2])) / 100
    const l = Math.min(100, Number(hslMatch[3])) / 100
    const hue2rgb = (p: number, q: number, t: number) => {
      let tt = t
      if (tt < 0) tt += 1
      if (tt > 1) tt -= 1
      if (tt < 1 / 6) return p + (q - p) * 6 * tt
      if (tt < 1 / 2) return q
      if (tt < 2 / 3) return p + (q - p) * (2 / 3 - tt) * 6
      return p
    }
    let r = l
    let g = l
    let b = l
    if (sat !== 0) {
      const q = l < 0.5 ? l * (1 + sat) : l + sat - l * sat
      const p = 2 * l - q
      r = hue2rgb(p, q, h + 1 / 3)
      g = hue2rgb(p, q, h)
      b = hue2rgb(p, q, h - 1 / 3)
    }
    return { r: Math.round(r * 255), g: Math.round(g * 255), b: Math.round(b * 255) }
  }
  return null
}

const colorConversionCards = computed(() => {
  const rgb = parseAnyColorToRgb(colorInputStr.value)
  if (!rgb) return null
  const { r, g, b } = rgb
  const toHex = (n: number) => n.toString(16).padStart(2, '0').toUpperCase()
  const hexUpper = `#${toHex(r)}${toHex(g)}${toHex(b)}`
  const hexLower = hexUpper.toLowerCase()

  // RGB -> HSL
  const rn = r / 255
  const gn = g / 255
  const bn = b / 255
  const max = Math.max(rn, gn, bn)
  const min = Math.min(rn, gn, bn)
  let h = 0
  let s = 0
  const l = (max + min) / 2
  if (max !== min) {
    const d = max - min
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min)
    switch (max) {
      case rn:
        h = (gn - bn) / d + (gn < bn ? 6 : 0)
        break
      case gn:
        h = (bn - rn) / d + 2
        break
      case bn:
        h = (rn - gn) / d + 4
        break
    }
    h /= 6
  }
  const hDeg = Math.round(h * 360)
  const sPct = Math.round(s * 100)
  const lPct = Math.round(l * 100)

  // RGB -> CMYK
  const k = 1 - max
  const c = k === 1 ? 0 : Math.round(((1 - rn - k) / (1 - k)) * 100)
  const m = k === 1 ? 0 : Math.round(((1 - gn - k) / (1 - k)) * 100)
  const y = k === 1 ? 0 : Math.round(((1 - bn - k) / (1 - k)) * 100)
  const kPct = Math.round(k * 100)

  return {
    hexUpper,
    cards: [
      { key: 'col-hex', label: 'HEX (大写)', value: hexUpper, sub: '网页与设计稿标准色值' },
      { key: 'col-hex-l', label: 'HEX (小写)', value: hexLower, sub: 'CSS 小写十六进制' },
      { key: 'col-rgb', label: 'RGB 格式', value: `rgb(${r}, ${g}, ${b})`, sub: `R:${r} G:${g} B:${b}` },
      {
        key: 'col-rgba',
        label: 'RGBA (含透明度)',
        value: `rgba(${r}, ${g}, ${b}, 1)`,
        sub: '支持 Alpha 通道叠加'
      },
      {
        key: 'col-hsl',
        label: 'HSL 色相饱和度',
        value: `hsl(${hDeg}, ${sPct}%, ${lPct}%)`,
        sub: `色相 ${hDeg}° · 饱和度 ${sPct}% · 亮度 ${lPct}%`
      },
      {
        key: 'col-cmyk',
        label: 'CMYK 印刷四色',
        value: `cmyk(${c}%, ${m}%, ${y}%, ${kPct}%)`,
        sub: `C:${c} M:${m} Y:${y} K:${kPct}`
      },
      {
        key: 'col-css-var',
        label: 'CSS / Tailwind 变量值',
        value: `${r} ${g} ${b}`,
        sub: '用于 rgb(var(--color) / <alpha>)'
      },
      {
        key: 'col-int',
        label: '十进制颜色整数',
        value: String((r << 16) + (g << 8) + b),
        sub: `0x${toHex(r)}${toHex(g)}${toHex(b)}`
      }
    ]
  }
})

// --- 3.3 Text Encoding & Hash Digest (Base64 / URL / Unicode / JWT / SHA-256) ---
const encodingInput = ref<string>('OmniBox 极速换算与格式工作台')
const sha256Hex = ref<string>('')
const sha1Hex = ref<string>('')

function utf8ToBase64(str: string): string {
  try {
    const bytes = new TextEncoder().encode(str)
    let binStr = ''
    bytes.forEach((b) => {
      binStr += String.fromCharCode(b)
    })
    return btoa(binStr)
  } catch {
    return '--'
  }
}

function base64ToUtf8(b64: string): string {
  try {
    const clean = b64.trim().replace(/-/g, '+').replace(/_/g, '/')
    const binStr = atob(clean)
    const bytes = Uint8Array.from(binStr, (m) => m.charCodeAt(0))
    return new TextDecoder().decode(bytes)
  } catch {
    return '（当前输入不是合法的 Base64 密文）'
  }
}

function toUnicodeEscape(str: string): string {
  return str
    .split('')
    .map((ch) => {
      const code = ch.charCodeAt(0)
      return code > 127 ? `\\u${code.toString(16).padStart(4, '0')}` : ch
    })
    .join('')
}

function fromUnicodeEscape(str: string): string {
  try {
    return str.replace(/\\u([0-9a-fA-F]{4})/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)))
  } catch {
    return str
  }
}

async function updateCryptoHashes(text: string) {
  if (!text || typeof crypto === 'undefined' || !crypto.subtle) {
    sha256Hex.value = '--'
    sha1Hex.value = '--'
    return
  }
  try {
    const data = new TextEncoder().encode(text)
    const [buf256, buf1] = await Promise.all([
      crypto.subtle.digest('SHA-256', data),
      crypto.subtle.digest('SHA-1', data)
    ])
    const toHex = (buf: ArrayBuffer) =>
      Array.from(new Uint8Array(buf))
        .map((b) => b.toString(16).padStart(2, '0'))
        .join('')
    sha256Hex.value = toHex(buf256)
    sha1Hex.value = toHex(buf1)
  } catch {
    sha256Hex.value = '--'
    sha1Hex.value = '--'
  }
}

watch(
  encodingInput,
  (val) => {
    updateCryptoHashes(val)
  },
  { immediate: true }
)

const encodingCards = computed(() => {
  const s = encodingInput.value
  if (!s) return []
  let urlDec = s
  try {
    urlDec = decodeURIComponent(s)
  } catch {
    urlDec = '（无法解码）'
  }
  return [
    { key: 'enc-b64-e', label: 'Base64 编码结果', value: utf8ToBase64(s) },
    { key: 'enc-b64-d', label: 'Base64 解码结果', value: base64ToUtf8(s) },
    { key: 'enc-url-e', label: 'URL Encode 编码', value: encodeURIComponent(s) },
    { key: 'enc-url-d', label: 'URL Decode 解码', value: urlDec },
    {
      key: 'enc-uni',
      label: s.includes('\\u') ? 'Unicode 转中文结果' : '中文转 Unicode (\\uXXXX)',
      value: s.includes('\\u') ? fromUnicodeEscape(s) : toUnicodeEscape(s)
    },
    { key: 'enc-sha256', label: 'SHA-256 哈希摘要', value: sha256Hex.value },
    { key: 'enc-sha1', label: 'SHA-1 哈希摘要', value: sha1Hex.value },
    {
      key: 'enc-len',
      label: '字符与 UTF-8 字节统计',
      value: `${s.length} 字符 / ${new TextEncoder().encode(s).length} Bytes`
    }
  ]
})

// --- 3.4 Structured Data Format Converter (URL Query / .env / CSV <-> JSON & Markdown Table) ---
type DataConvertSubMode = 'query_json' | 'env_json' | 'csv_md'
const dataConvertMode = ref<DataConvertSubMode>('query_json')
const dataConvertInput = ref<string>('page=1&pageSize=20&keyword=OmniBox&sort=desc')

function selectDataConvertSample(mode: DataConvertSubMode) {
  dataConvertMode.value = mode
  if (mode === 'query_json') {
    dataConvertInput.value = 'page=1&pageSize=20&keyword=OmniBox&sort=desc'
  } else if (mode === 'env_json') {
    dataConvertInput.value = 'PORT=8080\nDB_HOST=127.0.0.1\nENABLE_CACHE=true\nAPP_NAME=OmniBox'
  } else {
    dataConvertInput.value = '姓名,部门,工号\n张三,基础架构组,1001\n李四,AI研发部,1002'
  }
}

const dataConvertOutput = computed(() => {
  const raw = dataConvertInput.value.trim()
  if (!raw) return ''

  if (dataConvertMode.value === 'query_json') {
    // Auto bidirectional: if starts with `{`, convert JSON -> QueryString; else QueryString -> JSON
    if (raw.startsWith('{')) {
      try {
        const obj = JSON.parse(raw)
        const params = new URLSearchParams()
        for (const [k, v] of Object.entries(obj)) {
          params.append(k, String(v))
        }
        return decodeURIComponent(params.toString())
      } catch (e) {
        return `JSON 解析错误: ${String(e)}`
      }
    } else {
      const clean = raw.replace(/^[?#]/, '').replace(/^https?:\/\/[^?]+\?/, '')
      const params = new URLSearchParams(clean)
      const res: Record<string, string | number | boolean> = {}
      params.forEach((v, k) => {
        if (v === 'true') res[k] = true
        else if (v === 'false') res[k] = false
        else if (/^-?\d+(\.\d+)?$/.test(v)) res[k] = Number(v)
        else res[k] = v
      })
      return JSON.stringify(res, null, 2)
    }
  }

  if (dataConvertMode.value === 'env_json') {
    if (raw.startsWith('{')) {
      try {
        const obj = JSON.parse(raw)
        return Object.entries(obj)
          .map(([k, v]) => `${k}=${String(v)}`)
          .join('\n')
      } catch (e) {
        return `JSON 解析错误: ${String(e)}`
      }
    } else {
      const res: Record<string, string | number | boolean> = {}
      for (const line of raw.split(/\r?\n/)) {
        const trimmed = line.trim()
        if (!trimmed || trimmed.startsWith('#')) continue
        const idx = trimmed.indexOf('=')
        if (idx > 0) {
          const k = trimmed.slice(0, idx).trim()
          const v = trimmed
            .slice(idx + 1)
            .trim()
            .replace(/^['"]|['"]$/g, '')
          if (v === 'true') res[k] = true
          else if (v === 'false') res[k] = false
          else if (/^-?\d+(\.\d+)?$/.test(v)) res[k] = Number(v)
          else res[k] = v
        }
      }
      return JSON.stringify(res, null, 2)
    }
  }

  // CSV / TSV -> Markdown Table
  const lines = raw.split(/\r?\n/).filter((l) => l.trim().length > 0)
  if (lines.length === 0) return ''
  const sep = lines[0].includes('\t') ? '\t' : ','
  const rows = lines.map((l) => l.split(sep).map((c) => c.trim()))
  const header = rows[0]
  const divider = header.map(() => '---')
  const mdLines = [
    `| ${header.join(' | ')} |`,
    `| ${divider.join(' | ')} |`,
    ...rows.slice(1).map((r) => `| ${r.join(' | ')} |`)
  ]
  return mdLines.join('\n')
})

// ============================================================================
// 4. React to Initial Payload or Sub-Input Search from OmniBox Header
// ============================================================================
function applyExternalPayload(raw: string) {
  const clean = raw.trim()
  if (!clean) return

  // 1. Check HEX / RGB Color
  if (/^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(clean) || /^rgba?\(/i.test(clean)) {
    activeMode.value = 'format'
    activeFormatTool.value = 'color'
    colorInputStr.value = clean
    return
  }

  // 2. Check 10-digit or 13-digit Unix Timestamp
  if (/^\d{10}$/.test(clean) || /^\d{13}$/.test(clean)) {
    activeMode.value = 'format'
    activeFormatTool.value = 'timestamp'
    tsInputStr.value = clean
    return
  }

  // 3. Check RMB uppercase request (e.g. `12345大写` or `rmb:12345`)
  if (clean.includes('大写') || clean.toLowerCase().startsWith('rmb')) {
    const numMatch = clean.match(/\d+(\.\d+)?/)
    activeMode.value = 'unit'
    activeCategoryId.value = 'base_rmb'
    if (numMatch) baseInputStr.value = numMatch[0]
    return
  }

  // 4. Check Unit Expression (e.g., `500g`, `1024mb`, `100亩`, `2.5bar`, `1800kj`)
  const unitMatch = clean.match(/^(-?\d+(?:\.\d+)?)\s*([a-zA-Z\u4e00-\u9fa5°²³]+)/)
  if (unitMatch) {
    const numStr = unitMatch[1]
    const token = unitMatch[2].toLowerCase()
    const tokenMap: Record<string, { cat: UnitCategoryId; unit: string }> = {
      b: { cat: 'data', unit: 'B' },
      kb: { cat: 'data', unit: 'KB' },
      mb: { cat: 'data', unit: 'MB' },
      gb: { cat: 'data', unit: 'GB' },
      tb: { cat: 'data', unit: 'TB' },
      gib: { cat: 'data', unit: 'GB' },
      mbps: { cat: 'bandwidth', unit: 'Mbps' },
      gbps: { cat: 'bandwidth', unit: 'Gbps' },
      km: { cat: 'length', unit: 'km' },
      m: { cat: 'length', unit: 'm' },
      米: { cat: 'length', unit: 'm' },
      cm: { cat: 'length', unit: 'cm' },
      厘米: { cat: 'length', unit: 'cm' },
      mm: { cat: 'length', unit: 'mm' },
      in: { cat: 'length', unit: 'in' },
      英寸: { cat: 'length', unit: 'in' },
      ft: { cat: 'length', unit: 'ft' },
      英尺: { cat: 'length', unit: 'ft' },
      尺: { cat: 'length', unit: 'chi' },
      里: { cat: 'length', unit: 'li' },
      亩: { cat: 'area', unit: 'mu' },
      坪: { cat: 'area', unit: 'ping' },
      平: { cat: 'area', unit: 'm2' },
      平方米: { cat: 'area', unit: 'm2' },
      公顷: { cat: 'area', unit: 'ha' },
      l: { cat: 'volume', unit: 'L' },
      升: { cat: 'volume', unit: 'L' },
      ml: { cat: 'volume', unit: 'mL' },
      毫升: { cat: 'volume', unit: 'mL' },
      gal: { cat: 'volume', unit: 'gal_us' },
      kg: { cat: 'mass', unit: 'kg' },
      公斤: { cat: 'mass', unit: 'kg' },
      g: { cat: 'mass', unit: 'g' },
      克: { cat: 'mass', unit: 'g' },
      斤: { cat: 'mass', unit: 'jin' },
      两: { cat: 'mass', unit: 'liang' },
      lb: { cat: 'mass', unit: 'lb' },
      磅: { cat: 'mass', unit: 'lb' },
      oz: { cat: 'mass', unit: 'oz' },
      盎司: { cat: 'mass', unit: 'oz' },
      c: { cat: 'temperature', unit: 'C' },
      f: { cat: 'temperature', unit: 'F' },
      华氏度: { cat: 'temperature', unit: 'F' },
      摄氏度: { cat: 'temperature', unit: 'C' },
      s: { cat: 'time', unit: 's' },
      秒: { cat: 'time', unit: 's' },
      min: { cat: 'time', unit: 'min' },
      分钟: { cat: 'time', unit: 'min' },
      h: { cat: 'time', unit: 'h' },
      小时: { cat: 'time', unit: 'h' },
      kmh: { cat: 'speed', unit: 'kmh' },
      mph: { cat: 'speed', unit: 'mph' },
      kj: { cat: 'energy', unit: 'kJ' },
      千焦: { cat: 'energy', unit: 'kJ' },
      kcal: { cat: 'energy', unit: 'kcal' },
      大卡: { cat: 'energy', unit: 'kcal' },
      bar: { cat: 'pressure', unit: 'bar' },
      psi: { cat: 'pressure', unit: 'psi' },
      kpa: { cat: 'pressure', unit: 'kPa' }
    }
    const found = tokenMap[token]
    if (found) {
      activeMode.value = 'unit'
      activeCategoryId.value = found.cat
      unitSourceId.value = found.unit
      unitInputVal.value = numStr
    }
  }
}

watch(
  () => store.subInputQuery,
  (val) => {
    if (val.trim()) {
      applyExternalPayload(val)
    }
  }
)

onMounted(() => {
  clockTimer = setInterval(() => {
    liveNowMs.value = Date.now()
  }, 1000)

  if (store.pluginInitialPayload) {
    const payload = store.pluginInitialPayload
    store.pluginInitialPayload = ''
    applyExternalPayload(payload)
  }
})

onUnmounted(() => {
  if (clockTimer) clearInterval(clockTimer)
})
</script>

<template>
  <div
    class="flex flex-col flex-1 h-full min-h-[480px] overflow-hidden select-none"
    :class="store.isDark ? 'bg-[#0E131F] text-slate-100' : 'bg-[#F8FAFC] text-slate-800'"
  >
    <!-- ===================================================================== -->
    <!-- Top Segmented Navigation Bar (Fixed Height 48px, Zero Layout Shift)   -->
    <!-- ===================================================================== -->
    <div
      class="flex items-center justify-between h-[48px] px-5 border-b shrink-0 gap-3"
      :class="
        store.isDark ? 'bg-[#0A0E17]/90 border-slate-800/90' : 'bg-white border-slate-200/85'
      "
    >
      <div
        class="flex items-center gap-1 p-1 rounded-xl border shrink-0"
        :class="
          store.isDark
            ? 'bg-slate-900/90 border-slate-800'
            : 'bg-slate-100/90 border-slate-200/70'
        "
      >
        <button
          type="button"
          class="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold whitespace-nowrap shrink-0 cursor-pointer transition-all"
          :class="
            activeMode === 'unit'
              ? store.isDark
                ? 'bg-indigo-500/20 text-indigo-300 shadow-xs'
                : 'bg-white text-indigo-600 shadow-xs'
              : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
          "
          @click="activeMode = 'unit'"
        >
          <IconRenderer name="ArrowLeftRight" :size="13" />
          <span>度量衡单位换算</span>
        </button>

        <button
          type="button"
          class="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold whitespace-nowrap shrink-0 cursor-pointer transition-all"
          :class="
            activeMode === 'format'
              ? store.isDark
                ? 'bg-indigo-500/20 text-indigo-300 shadow-xs'
                : 'bg-white text-indigo-600 shadow-xs'
              : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
          "
          @click="activeMode = 'format'"
        >
          <IconRenderer name="Wand2" :size="13" />
          <span>编码与格式转换</span>
        </button>
      </div>

      <!-- Right Context Controls (Consistent height & dropdown layout) -->
      <div v-if="activeMode === 'unit'" class="flex items-center gap-2 shrink-0">
        <span class="text-[11px] text-slate-400 whitespace-nowrap">快速示例:</span>
        <select
          aria-label="选择当前分类预设值"
          class="h-[28px] px-2.5 rounded-lg border text-[11.5px] font-medium outline-none cursor-pointer"
          :class="
            store.isDark
              ? 'bg-slate-900 border-slate-700 text-slate-200'
              : 'bg-white border-slate-200 text-slate-700'
          "
          @change="
            (e) => {
              const idx = Number((e.target as HTMLSelectElement).value)
              const p = activeCategory.presets[idx]
              if (p) {
                if (activeCategory.id === 'base_rmb') {
                  baseInputStr = p.val
                } else {
                  applyUnitPreset(p.val, p.unit)
                }
              }
              ;(e.target as HTMLSelectElement).value = ''
            }
          "
        >
          <option value="" disabled selected>选择常用{{ activeCategory.shortLabel }}锚点...</option>
          <option v-for="(p, idx) in activeCategory.presets" :key="p.label" :value="idx">
            {{ p.label }}
          </option>
        </select>
      </div>

      <div v-else class="flex items-center gap-1.5 shrink-0">
        <button
          v-for="tool in [
            { id: 'timestamp', label: '时间戳/日期' },
            { id: 'color', label: '颜色 HEX/RGB' },
            { id: 'encoding', label: 'Base64/URL/哈希' },
            { id: 'data_format', label: '参数/表格互转' }
          ]"
          :key="tool.id"
          type="button"
          class="h-[28px] px-2.5 rounded-lg border text-[11.5px] font-medium whitespace-nowrap cursor-pointer transition-all"
          :class="
            activeFormatTool === tool.id
              ? store.isDark
                ? 'bg-indigo-500/20 border-indigo-500/40 text-indigo-300 font-semibold'
                : 'bg-indigo-600 border-indigo-600 text-white font-semibold'
              : store.isDark
                ? 'bg-slate-900 border-slate-800 text-slate-400 hover:text-slate-200'
                : 'bg-white border-slate-200 text-slate-600 hover:bg-slate-50'
          "
          @click="activeFormatTool = tool.id as FormatToolId"
        >
          {{ tool.label }}
        </button>
      </div>
    </div>

    <!-- ===================================================================== -->
    <!-- TAB 1: 度量衡单位换算 (10 Categories + One-to-Many Live Matrix)       -->
    <!-- ===================================================================== -->
    <div v-if="activeMode === 'unit'" class="flex-1 overflow-y-auto px-5 py-3.5 space-y-3.5">
      <!-- Category Pill Selector Row -->
      <div class="flex items-center gap-1.5 flex-wrap">
        <button
          v-for="cat in UNIT_CATEGORIES"
          :key="cat.id"
          type="button"
          class="px-2.5 py-1 rounded-lg border text-xs font-medium whitespace-nowrap cursor-pointer transition-all"
          :class="
            activeCategoryId === cat.id
              ? store.isDark
                ? 'bg-indigo-500/25 border-indigo-500 text-indigo-200 font-bold shadow-xs'
                : 'bg-indigo-600 border-indigo-600 text-white font-bold shadow-xs'
              : store.isDark
                ? 'bg-[#121929]/80 border-slate-800 text-slate-400 hover:text-slate-200'
                : 'bg-white border-slate-200/85 text-slate-600 hover:bg-slate-50'
          "
          @click="selectUnitCategory(cat.id)"
        >
          {{ cat.shortLabel }}
        </button>
      </div>

      <!-- Case A: Standard Physical / Digital Unit Matrix -->
      <template v-if="activeCategory.id !== 'base_rmb'">
        <!-- Primary Input Bar -->
        <div
          class="p-3.5 rounded-2xl border space-y-2.5"
          :class="
            store.isDark
              ? 'bg-[#121929]/90 border-slate-800/90'
              : 'bg-white border-slate-200/85 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
          "
        >
          <div class="flex items-center gap-2 min-w-0">
            <span class="text-xs font-bold whitespace-nowrap shrink-0">{{ activeCategory.shortLabel }}:</span>
            <input
              v-model="unitInputVal"
              type="text"
              placeholder="输入待换算数值..."
              aria-label="输入待换算数值"
              class="w-[150px] shrink-0 h-[34px] px-3 rounded-xl border font-mono text-sm font-bold outline-none"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-700 text-white focus:border-indigo-500'
                  : 'bg-slate-50 border-slate-300 text-slate-900 focus:border-indigo-500'
              "
            />
            <select
              v-model="unitSourceId"
              aria-label="选择基准源单位"
              class="h-[34px] max-w-[175px] shrink-0 px-2.5 rounded-xl border text-xs font-semibold outline-none cursor-pointer truncate"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-700 text-indigo-300'
                  : 'bg-slate-50 border-slate-300 text-indigo-600'
              "
            >
              <option v-for="u in activeCategory.units" :key="u.id" :value="u.id">
                {{ u.name.replace(/\s*\([^)]*\)/g, '') }} ({{ u.symbol }})
              </option>
            </select>

            <!-- 1024 vs 1000 Radix Switcher exclusively for Data Storage -->
            <div
              v-if="activeCategory.id === 'data'"
              class="flex items-center gap-1 p-0.5 rounded-xl border ml-auto shrink-0"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-800'
                  : 'bg-slate-100 border-slate-200/80'
              "
            >
              <button
                type="button"
                class="px-2 py-1 rounded-lg text-[11px] font-semibold whitespace-nowrap cursor-pointer transition-all"
                :class="
                  dataStorageRadix === 1024
                    ? store.isDark
                      ? 'bg-indigo-500/25 text-indigo-300'
                      : 'bg-white text-indigo-600 shadow-xs'
                    : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
                "
                @click="dataStorageRadix = 1024"
              >
                1024 进制 (Win/内存/编程)
              </button>
              <button
                type="button"
                class="px-2 py-1 rounded-lg text-[11px] font-semibold whitespace-nowrap cursor-pointer transition-all"
                :class="
                  dataStorageRadix === 1000
                    ? store.isDark
                      ? 'bg-indigo-500/25 text-indigo-300'
                      : 'bg-white text-indigo-600 shadow-xs'
                    : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-200'
                "
                @click="dataStorageRadix = 1000"
              >
                1000 进制 (macOS/硬盘)
              </button>
            </div>

            <span
              v-else
              class="text-[11px] text-slate-400 ml-auto truncate hidden sm:inline"
            >
              点击下方任意单位卡片可直接复制结果
            </span>
          </div>

          <!-- Disk Capacity Shrinkage Insight Banner (1000 Vendor vs 1024 OS) -->
          <div
            v-if="diskCapacityShrinkInsight"
            class="px-3 py-1.5 rounded-xl border text-[11.5px] font-medium flex items-center justify-between gap-2 cursor-pointer transition-colors"
            :class="[
              isCopied('disk-shrink-val')
                ? 'border-emerald-500/70 bg-emerald-500/10 text-emerald-400'
                : store.isDark
                  ? 'bg-indigo-500/10 border-indigo-500/25 text-indigo-300 hover:border-indigo-500/45'
                  : 'bg-indigo-50/70 border-indigo-200/70 text-indigo-700 hover:border-indigo-300'
            ]"
            @click="
              copyField(
                diskCapacityShrinkInsight.osRaw,
                `硬盘实际容量 (${diskCapacityShrinkInsight.osValue})`,
                'disk-shrink-val'
              )
            "
          >
            <div class="flex items-center gap-2 min-w-0">
              <IconRenderer name="Database" :size="13" class-name="shrink-0" />
              <span class="truncate">{{ diskCapacityShrinkInsight.text }}</span>
            </div>
            <span class="text-[10.5px] whitespace-nowrap shrink-0 underline opacity-80">
              {{ isCopied('disk-shrink-val') ? '✓ 已复制' : '复制实际值' }}
            </span>
          </div>

          <!-- Smart Contextual Insight Banner (Bandwidth ETA / Compound Duration / Marathon Pace) -->
          <div
            v-if="categoryInsightBanner"
            class="px-3 py-1.5 rounded-xl border text-[11.5px] font-medium flex items-center gap-2"
            :class="
              store.isDark
                ? 'bg-indigo-500/10 border-indigo-500/25 text-indigo-300'
                : 'bg-indigo-50/70 border-indigo-200/70 text-indigo-700'
            "
          >
            <IconRenderer name="Sparkles" :size="13" class-name="shrink-0" />
            <span class="truncate">{{ categoryInsightBanner }}</span>
          </div>
        </div>

        <!-- All Units Grid (4 Columns, One-to-Many Live Matrix) -->
        <div class="grid grid-cols-4 gap-2.5 text-xs">
          <div
            v-for="item in computedUnitMatrix"
            :key="item.id"
            class="p-2.5 rounded-xl border flex flex-col justify-between cursor-pointer transition-all"
            :class="[
              isCopied(`unit-${item.id}`)
                ? 'border-emerald-500/70 bg-emerald-500/10'
                : item.id === unitSourceId
                  ? store.isDark
                    ? 'bg-indigo-500/12 border-indigo-500/50'
                    : 'bg-indigo-50/50 border-indigo-300'
                  : store.isDark
                    ? 'bg-[#121929]/85 border-slate-800/90 hover:border-indigo-500/40'
                    : 'bg-white border-slate-200/85 hover:border-indigo-300 shadow-[0_1px_2px_rgba(15,23,42,0.02)]'
            ]"
            @click="copyField(item.formatted, `${item.name} (${item.symbol})`, `unit-${item.id}`)"
          >
            <div class="flex items-center justify-between gap-1">
              <span class="text-[11px] font-medium text-slate-400 truncate">{{ item.name }}</span>
              <IconRenderer
                :name="isCopied(`unit-${item.id}`) ? 'Check' : 'Copy'"
                :size="11"
                :class-name="
                  isCopied(`unit-${item.id}`)
                    ? 'text-emerald-400 shrink-0'
                    : 'text-slate-400 shrink-0'
                "
              />
            </div>

            <div class="font-mono text-[13.5px] font-bold tracking-tight my-1 truncate select-text">
              {{ item.formatted }}
              <span class="text-[11px] font-normal text-indigo-400 ml-0.5">{{ item.symbol }}</span>
            </div>

            <div class="flex items-center justify-between text-[10px] text-slate-400">
              <span class="truncate">{{ item.group }}</span>
              <button
                v-if="item.id !== unitSourceId && item.raw"
                type="button"
                class="text-indigo-400 hover:underline cursor-pointer shrink-0"
                @click.stop="applyUnitPreset(item.raw, item.id)"
              >
                设为基准
              </button>
              <span v-else-if="item.id === unitSourceId" class="text-indigo-400 font-semibold">
                当前基准
              </span>
            </div>
          </div>
        </div>
      </template>

      <!-- Case B: Number Base & Chinese Financial RMB Uppercase (`base_rmb`) -->
      <template v-else>
        <div
          class="p-4 rounded-2xl border space-y-3"
          :class="
            store.isDark
              ? 'bg-[#121929]/90 border-slate-800/90'
              : 'bg-white border-slate-200/85 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
          "
        >
          <div class="flex items-center gap-2.5">
            <span class="text-xs font-bold whitespace-nowrap">输入数字 / 金额:</span>
            <select
              v-model.number="baseInputRadix"
              aria-label="选择输入进制"
              class="h-[34px] px-2.5 rounded-xl border text-xs font-semibold outline-none cursor-pointer"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-700 text-indigo-300'
                  : 'bg-slate-50 border-slate-300 text-indigo-600'
              "
            >
              <option :value="10">十进制 (DEC / 金额)</option>
              <option :value="16">十六进制 (HEX)</option>
              <option :value="2">二进制 (BIN)</option>
              <option :value="8">八进制 (OCT)</option>
            </select>

            <input
              v-model="baseInputStr"
              type="text"
              placeholder="输入十进制金额或任意进制整数..."
              aria-label="输入数字或金额"
              class="flex-1 h-[34px] px-3 rounded-xl border font-mono text-sm font-bold outline-none"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-700 text-white focus:border-indigo-500'
                  : 'bg-slate-50 border-slate-300 text-slate-900 focus:border-indigo-500'
              "
            />
          </div>

          <!-- Highlighted Chinese Financial Uppercase Card -->
          <div
            v-if="baseAndRmbResults"
            class="p-3 rounded-xl border flex items-center justify-between gap-3 cursor-pointer transition-colors"
            :class="[
              isCopied('rmb-upper')
                ? 'border-emerald-500/70 bg-emerald-500/10'
                : store.isDark
                  ? 'bg-slate-950/75 border-indigo-500/35'
                  : 'bg-indigo-50/50 border-indigo-200/80'
            ]"
            @click="copyField(baseAndRmbResults.rmbUpper, '财务大写金额', 'rmb-upper')"
          >
            <div class="min-w-0">
              <div class="text-[11px] text-slate-400">
                人民币财务报销大写金额 ({{ baseAndRmbResults.formattedCurrency }})
              </div>
              <div class="text-base font-bold tracking-wide text-indigo-400 mt-0.5 select-text">
                {{ baseAndRmbResults.rmbUpper }}
              </div>
            </div>
            <button
              type="button"
              class="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-semibold border shrink-0 cursor-pointer"
              :class="
                isCopied('rmb-upper')
                  ? 'bg-emerald-500 text-white border-emerald-500'
                  : store.isDark
                    ? 'bg-indigo-500/20 border-indigo-500/40 text-indigo-300'
                    : 'bg-indigo-600 text-white border-indigo-600'
              "
            >
              <IconRenderer :name="isCopied('rmb-upper') ? 'Check' : 'Copy'" :size="13" />
              <span>{{ isCopied('rmb-upper') ? '已复制' : '复制大写' }}</span>
            </button>
          </div>
        </div>

        <!-- Number Base Grid -->
        <div v-if="baseAndRmbResults" class="grid grid-cols-3 gap-2.5 text-xs">
          <div
            v-for="bCard in [
              { key: 'base-dec', label: '十进制 (DEC)', value: baseAndRmbResults.dec, sub: 'Base 10' },
              { key: 'base-hex', label: '十六进制 (HEX)', value: baseAndRmbResults.hex, sub: `无前缀: ${baseAndRmbResults.hexRaw}` },
              { key: 'base-oct', label: '八进制 (OCT)', value: baseAndRmbResults.oct, sub: 'Base 8' },
              { key: 'base-bin', label: '二进制 (BIN · 4位分组)', value: baseAndRmbResults.bin, sub: 'Base 2' },
              { key: 'base-36', label: '36 进制 (0-9A-Z)', value: baseAndRmbResults.base36, sub: '短链编码常用' },
              { key: 'base-cur', label: '千分位货币格式', value: baseAndRmbResults.formattedCurrency, sub: '标准财务报表格式' }
            ]"
            :key="bCard.key"
            class="p-3 rounded-xl border flex flex-col justify-between cursor-pointer transition-all"
            :class="[
              isCopied(bCard.key)
                ? 'border-emerald-500/70 bg-emerald-500/10'
                : store.isDark
                  ? 'bg-[#121929]/85 border-slate-800/90 hover:border-indigo-500/40'
                  : 'bg-white border-slate-200/85 hover:border-indigo-300'
            ]"
            @click="copyField(bCard.value, bCard.label, bCard.key)"
          >
            <div class="flex items-center justify-between">
              <span class="text-[11px] font-medium text-slate-400">{{ bCard.label }}</span>
              <IconRenderer
                :name="isCopied(bCard.key) ? 'Check' : 'Copy'"
                :size="11"
                :class-name="isCopied(bCard.key) ? 'text-emerald-400' : 'text-slate-400'"
              />
            </div>
            <div class="font-mono text-[13px] font-bold my-1 break-all select-text">
              {{ bCard.value }}
            </div>
            <div class="text-[10px] text-slate-400">{{ bCard.sub }}</div>
          </div>
        </div>
      </template>
    </div>

    <!-- ===================================================================== -->
    <!-- TAB 2: 编码与格式转换 (Timestamp / Color / Encoding / Data Format)    -->
    <!-- ===================================================================== -->
    <div v-else class="flex-1 overflow-y-auto px-5 py-4 space-y-4">
      <!-- 2.1 Timestamp & Multi-Timezone Date Converter -->
      <template v-if="activeFormatTool === 'timestamp'">
        <div
          class="p-4 rounded-2xl border space-y-3"
          :class="
            store.isDark
              ? 'bg-[#121929]/90 border-slate-800/90'
              : 'bg-white border-slate-200/85 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
          "
        >
          <!-- Live Current Unix Timestamp Bar -->
          <div class="flex items-center justify-between text-xs">
            <div class="flex items-center gap-2">
              <span class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
              <span class="text-slate-400">当前实时 Unix 时间戳:</span>
              <button
                type="button"
                class="font-mono font-bold text-sm text-indigo-400 hover:underline cursor-pointer"
                @click="copyField(Math.floor(liveNowMs / 1000), '当前秒级时间戳', 'live-sec')"
              >
                {{ Math.floor(liveNowMs / 1000) }}s
              </button>
              <span class="text-slate-500">·</span>
              <button
                type="button"
                class="font-mono text-xs text-slate-400 hover:text-indigo-400 cursor-pointer"
                @click="copyField(liveNowMs, '当前毫秒时间戳', 'live-ms')"
              >
                {{ liveNowMs }}ms
              </button>
            </div>

            <div class="flex items-center gap-1.5">
              <button
                type="button"
                class="px-2.5 py-1 rounded-lg border text-[11px] font-medium cursor-pointer"
                :class="
                  store.isDark
                    ? 'bg-slate-900 border-slate-700 text-slate-300 hover:border-indigo-500'
                    : 'bg-slate-50 border-slate-200 text-slate-700 hover:border-indigo-400'
                "
                @click="setTsInputToNow(false)"
              >
                填入当前秒
              </button>
              <button
                type="button"
                class="px-2.5 py-1 rounded-lg border text-[11px] font-medium cursor-pointer"
                :class="
                  store.isDark
                    ? 'bg-slate-900 border-slate-700 text-slate-300 hover:border-indigo-500'
                    : 'bg-slate-50 border-slate-200 text-slate-700 hover:border-indigo-400'
                "
                @click="setTsInputToNow(true)"
              >
                填入当前毫秒
              </button>
            </div>
          </div>

          <div class="flex items-center gap-2.5">
            <span class="text-xs font-bold whitespace-nowrap">时间戳或日期:</span>
            <input
              v-model="tsInputStr"
              type="text"
              placeholder="输入 10位秒 / 13位毫秒时间戳 或 YYYY-MM-DD HH:mm:ss..."
              aria-label="输入时间戳或日期字符串"
              class="flex-1 h-[36px] px-3 rounded-xl border font-mono text-sm font-bold outline-none"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-700 text-white focus:border-indigo-500'
                  : 'bg-slate-50 border-slate-300 text-slate-900 focus:border-indigo-500'
              "
            />
          </div>
        </div>

        <div v-if="parsedTimestampCards" class="grid grid-cols-4 gap-2.5 text-xs">
          <div
            v-for="card in parsedTimestampCards"
            :key="card.key"
            class="p-3 rounded-xl border flex flex-col justify-between cursor-pointer transition-all"
            :class="[
              isCopied(card.key)
                ? 'border-emerald-500/70 bg-emerald-500/10'
                : store.isDark
                  ? 'bg-[#121929]/85 border-slate-800/90 hover:border-indigo-500/40'
                  : 'bg-white border-slate-200/85 hover:border-indigo-300'
            ]"
            @click="copyField(card.value, card.label, card.key)"
          >
            <div class="flex items-center justify-between gap-1">
              <span class="text-[11px] font-medium text-slate-400 truncate">{{ card.label }}</span>
              <IconRenderer
                :name="isCopied(card.key) ? 'Check' : 'Copy'"
                :size="11"
                :class-name="isCopied(card.key) ? 'text-emerald-400' : 'text-slate-400'"
              />
            </div>
            <div class="font-mono text-[12.5px] font-bold tracking-tight my-1 break-all select-text">
              {{ card.value }}
            </div>
            <div class="text-[10px] text-slate-400 truncate">{{ card.sub }}</div>
          </div>
        </div>
      </template>

      <!-- 2.2 Color Format Converter -->
      <template v-else-if="activeFormatTool === 'color'">
        <div
          class="p-4 rounded-2xl border flex items-center justify-between gap-4"
          :class="
            store.isDark
              ? 'bg-[#121929]/90 border-slate-800/90'
              : 'bg-white border-slate-200/85 shadow-[0_1px_3px_rgba(15,23,42,0.03)]'
          "
        >
          <div class="flex items-center gap-3 flex-1">
            <input
              type="color"
              :value="colorConversionCards?.hexUpper || '#4F46E5'"
              aria-label="拾色器"
              class="w-10 h-10 rounded-xl border-0 cursor-pointer bg-transparent shrink-0"
              @input="(e) => (colorInputStr = (e.target as HTMLInputElement).value)"
            />
            <input
              v-model="colorInputStr"
              type="text"
              placeholder="输入 #4F46E5 或 rgb(79, 70, 229) 或 hsl(243, 75%, 59%)..."
              aria-label="输入颜色值"
              class="flex-1 h-[36px] px-3 rounded-xl border font-mono text-sm font-bold outline-none"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-700 text-white focus:border-indigo-500'
                  : 'bg-slate-50 border-slate-300 text-slate-900 focus:border-indigo-500'
              "
            />
          </div>

          <!-- Quick Brand Color Swatches -->
          <div class="flex items-center gap-1.5 shrink-0">
            <button
              v-for="swatch in ['#4F46E5', '#0EA5E9', '#10B981', '#F59E0B', '#EF4444', '#8B5CF6', '#0F172A']"
              :key="swatch"
              type="button"
              :style="{ backgroundColor: swatch }"
              :title="swatch"
              class="w-6 h-6 rounded-lg border border-white/20 cursor-pointer transition-transform hover:scale-110"
              @click="colorInputStr = swatch"
            ></button>
          </div>
        </div>

        <div v-if="colorConversionCards" class="grid grid-cols-4 gap-2.5 text-xs">
          <div
            v-for="card in colorConversionCards.cards"
            :key="card.key"
            class="p-3 rounded-xl border flex flex-col justify-between cursor-pointer transition-all"
            :class="[
              isCopied(card.key)
                ? 'border-emerald-500/70 bg-emerald-500/10'
                : store.isDark
                  ? 'bg-[#121929]/85 border-slate-800/90 hover:border-indigo-500/40'
                  : 'bg-white border-slate-200/85 hover:border-indigo-300'
            ]"
            @click="copyField(card.value, card.label, card.key)"
          >
            <div class="flex items-center justify-between gap-1">
              <span class="text-[11px] font-medium text-slate-400 truncate">{{ card.label }}</span>
              <IconRenderer
                :name="isCopied(card.key) ? 'Check' : 'Copy'"
                :size="11"
                :class-name="isCopied(card.key) ? 'text-emerald-400' : 'text-slate-400'"
              />
            </div>
            <div class="font-mono text-[12.5px] font-bold tracking-tight my-1 break-all select-text">
              {{ card.value }}
            </div>
            <div class="text-[10px] text-slate-400 truncate">{{ card.sub }}</div>
          </div>
        </div>
      </template>

      <!-- 2.3 Text Encoding & Hash Digest -->
      <template v-else-if="activeFormatTool === 'encoding'">
        <div
          class="p-3.5 rounded-2xl border space-y-2"
          :class="
            store.isDark ? 'bg-[#121929]/90 border-slate-800/90' : 'bg-white border-slate-200/85'
          "
        >
          <div class="flex items-center justify-between text-xs">
            <span class="font-bold">输入待编解码或计算哈希的文本:</span>
            <button
              type="button"
              class="text-[11px] text-indigo-400 hover:underline cursor-pointer"
              @click="encodingInput = ''"
            >
              清空输入
            </button>
          </div>
          <textarea
            v-model="encodingInput"
            rows="2"
            placeholder="输入任意文本、URL、Base64 密文或 Unicode (\u4e2d\u6587)..."
            class="w-full p-2.5 rounded-xl border font-mono text-xs outline-none resize-none"
            :class="
              store.isDark
                ? 'bg-slate-950 border-slate-700 text-slate-100 focus:border-indigo-500'
                : 'bg-slate-50 border-slate-200 text-slate-800 focus:border-indigo-500'
            "
          ></textarea>
        </div>

        <div class="grid grid-cols-2 gap-2.5 text-xs">
          <div
            v-for="card in encodingCards"
            :key="card.key"
            class="p-3 rounded-xl border flex flex-col justify-between cursor-pointer transition-all"
            :class="[
              isCopied(card.key)
                ? 'border-emerald-500/70 bg-emerald-500/10'
                : store.isDark
                  ? 'bg-[#121929]/85 border-slate-800/90 hover:border-indigo-500/40'
                  : 'bg-white border-slate-200/85 hover:border-indigo-300'
            ]"
            @click="copyField(card.value, card.label, card.key)"
          >
            <div class="flex items-center justify-between">
              <span class="text-[11px] font-medium text-slate-400">{{ card.label }}</span>
              <IconRenderer
                :name="isCopied(card.key) ? 'Check' : 'Copy'"
                :size="11"
                :class-name="isCopied(card.key) ? 'text-emerald-400' : 'text-slate-400'"
              />
            </div>
            <div class="font-mono text-xs font-bold mt-1 line-clamp-2 break-all select-text">
              {{ card.value }}
            </div>
          </div>
        </div>
      </template>

      <!-- 2.4 Structured Data Format Converter (URL Query / .env / CSV <-> JSON & Markdown Table) -->
      <template v-else>
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-1.5">
            <button
              v-for="m in [
                { id: 'query_json', label: 'URL Query 参数 ↔ JSON' },
                { id: 'env_json', label: '.env 环境变量 ↔ JSON' },
                { id: 'csv_md', label: 'CSV / Excel 表格 → Markdown 表格' }
              ]"
              :key="m.id"
              type="button"
              class="px-2.5 py-1 rounded-lg border text-xs font-medium cursor-pointer transition-colors"
              :class="
                dataConvertMode === m.id
                  ? store.isDark
                    ? 'bg-indigo-500/20 border-indigo-500 text-indigo-300 font-bold'
                    : 'bg-indigo-600 border-indigo-600 text-white font-bold'
                  : store.isDark
                    ? 'bg-slate-900 border-slate-800 text-slate-400'
                    : 'bg-white border-slate-200 text-slate-600'
              "
              @click="selectDataConvertSample(m.id as DataConvertSubMode)"
            >
              {{ m.label }}
            </button>
          </div>

          <button
            type="button"
            class="flex items-center gap-1 px-3 py-1 rounded-lg border text-xs font-semibold cursor-pointer"
            :class="
              isCopied('data-fmt-out')
                ? 'bg-emerald-500 text-white border-emerald-500'
                : store.isDark
                  ? 'bg-indigo-500/20 border-indigo-500/40 text-indigo-300'
                  : 'bg-indigo-600 text-white border-indigo-600'
            "
            @click="copyField(dataConvertOutput, '转换结果', 'data-fmt-out')"
          >
            <IconRenderer :name="isCopied('data-fmt-out') ? 'Check' : 'Copy'" :size="12" />
            <span>{{ isCopied('data-fmt-out') ? '已复制结果' : '复制转换结果' }}</span>
          </button>
        </div>

        <div class="grid grid-cols-2 gap-3 h-[320px]">
          <div class="flex flex-col space-y-1.5">
            <span class="text-[11px] text-slate-400">输入源内容:</span>
            <textarea
              v-model="dataConvertInput"
              class="flex-1 p-3 rounded-xl border font-mono text-xs outline-none resize-none"
              :class="
                store.isDark
                  ? 'bg-slate-950 border-slate-800 text-slate-100 focus:border-indigo-500'
                  : 'bg-white border-slate-200 text-slate-800 focus:border-indigo-500'
              "
            ></textarea>
          </div>

          <div class="flex flex-col space-y-1.5">
            <span class="text-[11px] text-slate-400">实时转换输出:</span>
            <textarea
              :value="dataConvertOutput"
              readonly
              class="flex-1 p-3 rounded-xl border font-mono text-xs outline-none resize-none select-text"
              :class="
                store.isDark
                  ? 'bg-[#121929]/90 border-slate-800 text-emerald-300'
                  : 'bg-slate-50 border-slate-200 text-slate-800'
              "
            ></textarea>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>
