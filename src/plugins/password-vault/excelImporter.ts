import type { VaultEntry, VaultEnvType, VaultSubAccount, VaultTemplateType } from '@/bridge/backend'

export type ExcelColumnField =
  | 'ignore'
  | 'env'
  | 'title'
  | 'moduleName'
  | 'category'
  | 'url'
  | 'publicAddress'
  | 'privateAddress'
  | 'role'
  | 'username'
  | 'password'
  | 'accessKeyId'
  | 'accessKeySecret'
  | 'extraSecret'
  | 'notes'

export interface ExcelColumnMeta {
  field: ExcelColumnField
  label: string
}

export const EXCEL_COLUMN_OPTIONS: ExcelColumnMeta[] = [
  { field: 'title', label: '平台 / 系统 / 组件名' },
  { field: 'env', label: '环境 (dev/test/prev/prod)' },
  { field: 'moduleName', label: '业务模块 / 子集群 (如 lobby)' },
  { field: 'url', label: '访问地址 (URL / IP)' },
  { field: 'publicAddress', label: '外网地址 (Pub 连接串)' },
  { field: 'privateAddress', label: '内网地址 (Private 连接串)' },
  { field: 'username', label: '登录账号 / 用户名' },
  { field: 'password', label: '密码原文' },
  { field: 'role', label: '账号角色 (管理员/只读等)' },
  { field: 'accessKeyId', label: 'AccessKey ID (AK)' },
  { field: 'accessKeySecret', label: 'AccessKey Secret (SK)' },
  { field: 'extraSecret', label: '其他密钥 / ACR密码 / Root密码' },
  { field: 'notes', label: '备注 / 用途说明' },
  { field: 'category', label: '所属分类' },
  { field: 'ignore', label: '忽略此列 (如序号)' }
]

/**
 * Parse TSV (Tab-Separated Values) copied from Excel, properly handling quoted multiline cells.
 */
export function parseExcelTsvRows(rawText: string): string[][] {
  const text = rawText.replace(/\r\n/g, '\n').replace(/\r/g, '\n').trim()
  if (!text) return []

  const rows: string[][] = []
  let currentRow: string[] = []
  let currentCell = ''
  let inQuotes = false

  for (let i = 0; i < text.length; i++) {
    const ch = text[i]
    const next = text[i + 1]

    if (inQuotes) {
      if (ch === '"' && next === '"') {
        currentCell += '"'
        i++
      } else if (ch === '"') {
        inQuotes = false
      } else {
        currentCell += ch
      }
    } else {
      if (ch === '"' && currentCell.length === 0) {
        inQuotes = true
      } else if (ch === '\t') {
        currentRow.push(cleanExcelCell(currentCell))
        currentCell = ''
      } else if (ch === '\n') {
        currentRow.push(cleanExcelCell(currentCell))
        if (currentRow.some((c) => c.length > 0)) {
          rows.push(currentRow)
        }
        currentRow = []
        currentCell = ''
      } else {
        currentCell += ch
      }
    }
  }

  currentRow.push(cleanExcelCell(currentCell))
  if (currentRow.some((c) => c.length > 0)) {
    rows.push(currentRow)
  }

  // Normalize column count across all rows
  const maxCols = rows.reduce((max, r) => Math.max(max, r.length), 0)
  return rows.map((r) => {
    while (r.length < maxCols) r.push('')
    return r
  })
}

function cleanExcelCell(val: string): string {
  // Replace internal line breaks inside a single Excel cell (e.g. host\n3306) with colon or space
  return val
    .trim()
    .replace(/\n+/g, (m, offset, str) => {
      const after = str.slice(offset + m.length).trim()
      if (/^\d{2,5}$/.test(after)) return ':'
      return ' '
    })
    .trim()
}

export function detectHeaderAndMapping(rows: string[][]): {
  hasHeader: boolean
  mapping: ExcelColumnField[]
} {
  if (rows.length === 0) return { hasHeader: false, mapping: [] }
  const firstRow = rows[0]
  const colCount = firstRow.length
  const mapping: ExcelColumnField[] = new Array(colCount).fill('ignore')

  let headerMatchedCount = 0

  for (let i = 0; i < colCount; i++) {
    const h = firstRow[i].toLowerCase().trim()
    if (!h) continue

    if (/^(序号|no|id|#)$/i.test(h)) {
      mapping[i] = 'ignore'
      headerMatchedCount++
    } else if (/环境|env/i.test(h)) {
      mapping[i] = 'env'
      headerMatchedCount++
    } else if (/^(type|平台|系统|组件|服务|名称|title)$/i.test(h)) {
      mapping[i] = 'title'
      headerMatchedCount++
    } else if (/业务|模块|集群|module/i.test(h)) {
      mapping[i] = 'moduleName'
      headerMatchedCount++
    } else if (/pub|外网|公网/i.test(h)) {
      mapping[i] = 'publicAddress'
      headerMatchedCount++
    } else if (/priv|内网|私网/i.test(h)) {
      mapping[i] = 'privateAddress'
      headerMatchedCount++
    } else if (/^(addr|address|url|地址|域名|连接|ip)$/i.test(h)) {
      mapping[i] = 'url'
      headerMatchedCount++
    } else if (/accesskey\s*id|^ak$/i.test(h)) {
      mapping[i] = 'accessKeyId'
      headerMatchedCount++
    } else if (/accesskey\s*secret|^sk$/i.test(h)) {
      mapping[i] = 'accessKeySecret'
      headerMatchedCount++
    } else if (/acr|其他|附加|extra|other/i.test(h)) {
      mapping[i] = 'extraSecret'
      headerMatchedCount++
    } else if (/^(username|user|帐号|账号|用户|登录名)$/i.test(h)) {
      mapping[i] = 'username'
      headerMatchedCount++
    } else if (/^(password|pwd|pass|密码)$/i.test(h)) {
      mapping[i] = 'password'
      headerMatchedCount++
    } else if (/^(notes|note|备注|说明|用途)$/i.test(h)) {
      mapping[i] = 'notes'
      headerMatchedCount++
    }
  }

  if (headerMatchedCount >= 2) {
    // Handle unnamed column right after '组件' or '环境' in DB tables (like screenshot 4 where 'lobby' has an empty header)
    for (let i = 0; i < colCount; i++) {
      if (mapping[i] === 'ignore' && !firstRow[i] && i > 0) {
        if (mapping[i - 1] === 'env' || mapping[i - 1] === 'title') {
          mapping[i] = 'moduleName'
        }
      }
    }
    return { hasHeader: true, mapping }
  }

  // Fallback heuristic when user copies rows without headers
  const fallbackOrder: ExcelColumnField[] = [
    'title',
    'url',
    'username',
    'password',
    'notes',
    'extraSecret'
  ]
  for (let i = 0; i < colCount; i++) {
    mapping[i] = fallbackOrder[i] || 'ignore'
  }
  return { hasHeader: false, mapping }
}

export function normalizeEnvValue(raw: string): VaultEnvType {
  const v = raw.toLowerCase().trim()
  if (!v) return 'COMMON'
  if (v.includes('prod') || v.includes('生产') || v.includes('线上')) return 'PROD'
  if (v.includes('prev') || v.includes('预发') || v.includes('stag') || v.includes('uat')) return 'PREV'
  if (v.includes('test') || v.includes('测试') || v.includes('qa')) return 'TEST'
  if (v.includes('dev') || v.includes('开发')) return 'DEV'
  return 'COMMON'
}

/**
 * Convert parsed Excel rows + column mapping into structured VaultEntry list,
 * automatically handling Excel merged cells (fill-down) and aggregating multi-account instances.
 */
export function buildVaultEntriesFromExcel(
  rows: string[][],
  mapping: ExcelColumnField[],
  hasHeader: boolean,
  defaultCategory = '研发与云',
  aggregateSameInstance = true
): VaultEntry[] {
  const dataRows = hasHeader ? rows.slice(1) : rows
  if (dataRows.length === 0) return []

  const getCol = (row: string[], field: ExcelColumnField): string => {
    const idx = mapping.indexOf(field)
    return idx >= 0 ? (row[idx] || '').trim() : ''
  }

  // Track previous row's parent context to handle Excel merged cells (where subsequent rows leave title/env/address blank)
  let lastTitle = ''
  let lastEnv = ''
  let lastModule = ''
  let lastUrl = ''
  let lastPub = ''
  let lastPriv = ''

  const entriesMap = new Map<string, VaultEntry>()
  const entriesList: VaultEntry[] = []

  dataRows.forEach((row, idx) => {
    let rawTitle = getCol(row, 'title')
    let rawEnv = getCol(row, 'env')
    let rawModule = getCol(row, 'moduleName')
    let rawUrl = getCol(row, 'url')
    let rawPub = getCol(row, 'publicAddress')
    let rawPriv = getCol(row, 'privateAddress')

    const username = getCol(row, 'username')
    const password = getCol(row, 'password')
    const ak = getCol(row, 'accessKeyId')
    const sk = getCol(row, 'accessKeySecret')
    const extra = getCol(row, 'extraSecret')
    const notes = getCol(row, 'notes')
    const role = getCol(row, 'role')
    const customCat = getCol(row, 'category')

    // Skip completely empty credential rows
    if (!rawTitle && !username && !password && !ak && !extra) {
      return
    }

    // Inherit merged cell values from the row above
    if (!rawTitle && lastTitle) rawTitle = lastTitle
    if (!rawEnv && lastEnv) rawEnv = lastEnv
    if (!rawModule && lastModule && rawTitle === lastTitle) rawModule = lastModule
    if (!rawUrl && lastUrl && rawTitle === lastTitle) rawUrl = lastUrl
    if (!rawPub && lastPub && rawTitle === lastTitle) rawPub = lastPub
    if (!rawPriv && lastPriv && rawTitle === lastTitle) rawPriv = lastPriv

    lastTitle = rawTitle
    lastEnv = rawEnv
    lastModule = rawModule
    lastUrl = rawUrl
    lastPub = rawPub
    lastPriv = rawPriv

    // Detect environment embedded in title or env column (e.g., "API (Prod)")
    const envType = normalizeEnvValue(rawEnv || rawTitle)

    // Determine template type & category automatically
    let templateType: VaultTemplateType = 'general'
    let category = customCat || defaultCategory

    const lowerTitle = rawTitle.toLowerCase()
    if (rawPub || rawPriv || /polardb|mysql|postgres|mongodb|redis|clickhouse|tidb|rds/i.test(lowerTitle)) {
      templateType = 'database'
      if (!customCat) category = '数据库中间件'
    } else if (ak || sk || /aliyun|aws|腾讯云|华为云|ram|iam|api/i.test(lowerTitle)) {
      templateType = 'cloud'
      if (!customCat) category = '云与API'
    } else if (/prometheus|grafana|doji|zentao|jira|confluence|gitlab|jenkins/i.test(lowerTitle)) {
      if (!customCat) category = '监控与后台'
    } else if (/vpn|ldap|服务器|跳板机|ssh/i.test(lowerTitle)) {
      if (!customCat) category = '基础设施'
    }

    const displayTitle = rawModule
      ? `${rawTitle || '未命名资源'} · ${rawModule}`
      : rawTitle || username || `导入条目 #${idx + 1}`

    const subAcc: VaultSubAccount = {
      id: `sub-${Date.now()}-${idx}-${Math.random().toString(36).slice(2, 6)}`,
      role: role || (notes && notes.length <= 14 ? notes : `账号 #1`),
      username,
      password,
      accessKeyId: ak || undefined,
      accessKeySecret: sk || undefined,
      extraSecret: extra || undefined,
      notes: notes || undefined
    }

    const groupKey = `${displayTitle}__${envType}__${rawUrl}__${rawPub}__${rawPriv}`

    if (aggregateSameInstance && entriesMap.has(groupKey)) {
      const existing = entriesMap.get(groupKey)!
      if (!existing.subAccounts) {
        existing.subAccounts = []
      }
      subAcc.role = role || (notes && notes.length <= 14 ? notes : `账号 #${existing.subAccounts.length + 1}`)
      existing.subAccounts.push(subAcc)
    } else {
      const newEntry: VaultEntry = {
        id: `v-imp-${Date.now()}-${idx}-${Math.random().toString(36).slice(2, 6)}`,
        title: displayTitle,
        username,
        password,
        url: rawUrl || rawPub || rawPriv || '',
        category,
        notes,
        updatedAt: Date.now(),
        templateType,
        env: envType,
        componentType: templateType === 'database' ? rawTitle : undefined,
        moduleName: rawModule || undefined,
        publicAddress: rawPub || undefined,
        privateAddress: rawPriv || undefined,
        accessKeyId: ak || undefined,
        accessKeySecret: sk || undefined,
        extraSecret: extra || undefined,
        subAccounts: [subAcc]
      }
      entriesMap.set(groupKey, newEntry)
      entriesList.push(newEntry)
    }
  })

  return entriesList
}
