import jsQR from 'jsqr'
import QRCode from 'qrcode'
import { decodeQrFromDataUrlNative, isTauriEnv, type TotpEntry } from '@/bridge/backend'

export interface ParsedOtpAccountItem {
  issuer: string
  account: string
  secret: string
  digits: number
  period: number
  category: string
}

export interface ParsedOtpPayload {
  ok: boolean
  isComplete: boolean
  issuer: string
  account: string
  secret: string
  digits: number
  period: number
  category: string
  rawUri: string
  /** Populated when decoding a Google Authenticator migration QR (`otpauth-migration://`) or multi-account payload */
  batchItems?: ParsedOtpAccountItem[]
  batchInfo?: {
    batchIndex: number
    batchSize: number
  }
  error?: string
}

const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'

/**
 * Convert raw binary secret bytes (`Uint8Array`) from Google Authenticator Protobuf into RFC 4648 Base32 string
 */
export function bytesToBase32(bytes: Uint8Array): string {
  if (!bytes || bytes.length === 0) return ''
  let bits = 0
  let value = 0
  let output = ''
  for (let i = 0; i < bytes.length; i++) {
    value = (value << 8) | bytes[i]!
    bits += 8
    while (bits >= 5) {
      output += BASE32_ALPHABET[(value >>> (bits - 5)) & 31]
      bits -= 5
    }
  }
  if (bits > 0) {
    output += BASE32_ALPHABET[(value << (5 - bits)) & 31]
  }
  return output
}

/**
 * Decode standard or URL-safe Base64 string into Uint8Array
 */
function decodeBase64ToBytes(rawB64: string): Uint8Array {
  // Handle unescaped spaces (from `+` in query param) and URL-safe `-` / `_`
  let normalized = rawB64
    .trim()
    .replace(/\s+/g, '+')
    .replace(/-/g, '+')
    .replace(/_/g, '/')
  const padMod = normalized.length % 4
  if (padMod === 2) normalized += '=='
  else if (padMod === 3) normalized += '='
  else if (padMod === 1) {
    throw new Error('Base64 长度非法')
  }

  const binStr = atob(normalized)
  const out = new Uint8Array(binStr.length)
  for (let i = 0; i < binStr.length; i++) {
    out[i] = binStr.charCodeAt(i)
  }
  return out
}

/**
 * Read a Protobuf base-128 Varint from `buf` starting at `offset`.
 * Returns `[value, nextOffset]`.
 */
function readProtoVarint(buf: Uint8Array, offset: number): [number, number] {
  let result = 0
  let shift = 0
  let pos = offset
  while (pos < buf.length) {
    const byte = buf[pos++]!
    result += (byte & 0x7f) * Math.pow(2, shift)
    if ((byte & 0x80) === 0) {
      return [result, pos]
    }
    shift += 7
    if (shift > 64) {
      throw new Error('Protobuf Varint 溢出')
    }
  }
  throw new Error('Protobuf 数据流意外截断')
}

/**
 * Skip an unknown Protobuf field based on its wireType
 */
function skipProtoField(buf: Uint8Array, offset: number, wireType: number): number {
  if (wireType === 0) {
    const [, nextPos] = readProtoVarint(buf, offset)
    return nextPos
  }
  if (wireType === 1) {
    return offset + 8
  }
  if (wireType === 2) {
    const [len, nextPos] = readProtoVarint(buf, offset)
    return nextPos + len
  }
  if (wireType === 5) {
    return offset + 4
  }
  throw new Error(`未知的 Protobuf WireType: ${wireType}`)
}

/**
 * Parse a single `MigrationPayload.OtpParameters` sub-message from Google Authenticator export
 */
function parseGoogleOtpParametersProto(subBuf: Uint8Array): ParsedOtpAccountItem | null {
  let pos = 0
  let secretBytes: Uint8Array | null = null
  let rawName = ''
  let rawIssuer = ''
  let digitsEnum = 1 // 1 = 6 digits, 2 = 8 digits

  const utf8Decoder = new TextDecoder('utf-8', { fatal: false })

  while (pos < subBuf.length) {
    const [tag, afterTag] = readProtoVarint(subBuf, pos)
    pos = afterTag
    const fieldNo = tag >>> 3
    const wireType = tag & 0x07

    if (fieldNo === 1 && wireType === 2) {
      // bytes secret = 1
      const [len, start] = readProtoVarint(subBuf, pos)
      secretBytes = subBuf.slice(start, start + len)
      pos = start + len
    } else if (fieldNo === 2 && wireType === 2) {
      // string name = 2
      const [len, start] = readProtoVarint(subBuf, pos)
      rawName = utf8Decoder.decode(subBuf.slice(start, start + len)).trim()
      pos = start + len
    } else if (fieldNo === 3 && wireType === 2) {
      // string issuer = 3
      const [len, start] = readProtoVarint(subBuf, pos)
      rawIssuer = utf8Decoder.decode(subBuf.slice(start, start + len)).trim()
      pos = start + len
    } else if (fieldNo === 5 && wireType === 0) {
      // DigitCount digits = 5 (1 -> 6 digits, 2 -> 8 digits)
      const [val, next] = readProtoVarint(subBuf, pos)
      digitsEnum = val
      pos = next
    } else {
      pos = skipProtoField(subBuf, pos, wireType)
    }
  }

  if (!secretBytes || secretBytes.length === 0) {
    return null
  }

  const base32Secret = sanitizeBase32Secret(bytesToBase32(secretBytes))
  if (!base32Secret || base32Secret.length < 6) {
    return null
  }

  let issuer = rawIssuer
  let account = rawName

  if (rawName.includes(':')) {
    const colonIdx = rawName.indexOf(':')
    const prefixIssuer = rawName.slice(0, colonIdx).trim()
    const restAccount = rawName.slice(colonIdx + 1).trim()
    if (!issuer && prefixIssuer) {
      issuer = prefixIssuer
    }
    account = restAccount || rawName
  }

  if (!issuer && account.includes('@')) {
    const domainPart = account.split('@')[1]?.split('.')[0] || ''
    if (domainPart) {
      issuer = domainPart.charAt(0).toUpperCase() + domainPart.slice(1)
    }
  }

  const finalIssuer = issuer || account || 'Google 导入令牌'
  const finalAccount = account || 'default@account'
  const digits = digitsEnum === 2 || digitsEnum === 8 ? 8 : 6

  return {
    issuer: finalIssuer,
    account: finalAccount,
    secret: base32Secret,
    digits,
    period: 30,
    category: inferCategoryFromIssuer(finalIssuer, finalAccount)
  }
}

/**
 * Decode `otpauth-migration://offline?data=...` URI from Google Authenticator Export QR Code
 */
export function parseGoogleMigrationUri(rawUri: string): ParsedOtpPayload {
  try {
    // Extract `data=` parameter robustly even if custom scheme parsing varies across WebViews
    let dataParam = ''
    const queryIdx = rawUri.indexOf('?')
    if (queryIdx !== -1) {
      const searchParams = new URLSearchParams(rawUri.slice(queryIdx + 1))
      dataParam = searchParams.get('data') || ''
    }
    if (!dataParam) {
      const match = rawUri.match(/[?&]data=([^&#\s]+)/i)
      if (match?.[1]) {
        try {
          dataParam = decodeURIComponent(match[1])
        } catch {
          dataParam = match[1]
        }
      }
    }

    if (!dataParam) {
      return {
        ok: false,
        isComplete: false,
        issuer: '',
        account: '',
        secret: '',
        digits: 6,
        period: 30,
        category: '开发平台',
        rawUri,
        error: 'Google 迁移二维码缺少 data 载荷参数'
      }
    }

    const buf = decodeBase64ToBytes(dataParam)
    const accounts: ParsedOtpAccountItem[] = []
    let batchSize = 1
    let batchIndex = 0
    let pos = 0

    while (pos < buf.length) {
      const [tag, afterTag] = readProtoVarint(buf, pos)
      pos = afterTag
      const fieldNo = tag >>> 3
      const wireType = tag & 0x07

      if (fieldNo === 1 && wireType === 2) {
        // repeated OtpParameters otp_parameters = 1
        const [len, start] = readProtoVarint(buf, pos)
        const subBytes = buf.slice(start, start + len)
        pos = start + len
        const parsedItem = parseGoogleOtpParametersProto(subBytes)
        if (parsedItem) {
          accounts.push(parsedItem)
        }
      } else if (fieldNo === 3 && wireType === 0) {
        const [val, next] = readProtoVarint(buf, pos)
        batchSize = val
        pos = next
      } else if (fieldNo === 4 && wireType === 0) {
        const [val, next] = readProtoVarint(buf, pos)
        batchIndex = val
        pos = next
      } else {
        pos = skipProtoField(buf, pos, wireType)
      }
    }

    if (accounts.length === 0) {
      return {
        ok: false,
        isComplete: false,
        issuer: '',
        account: '',
        secret: '',
        digits: 6,
        period: 30,
        category: '开发平台',
        rawUri,
        error: '未能从 Google Authenticator 迁移二维码中解析出有效的 TOTP 密钥'
      }
    }

    const first = accounts[0]!
    return {
      ok: true,
      isComplete: true,
      issuer: first.issuer,
      account: first.account,
      secret: first.secret,
      digits: first.digits,
      period: first.period,
      category: first.category,
      rawUri,
      batchItems: accounts,
      batchInfo: batchSize > 1 ? { batchIndex, batchSize } : undefined
    }
  } catch (e) {
    return {
      ok: false,
      isComplete: false,
      issuer: '',
      account: '',
      secret: '',
      digits: 6,
      period: 30,
      category: '开发平台',
      rawUri,
      error: `Google 迁移二维码解析失败：${String((e as Error)?.message || e)}`
    }
  }
}

/**
 * Infer a clean category tag from the issuer or account name
 */
export function inferCategoryFromIssuer(issuer: string, account = ''): string {
  const combined = `${issuer} ${account}`.toLowerCase()
  if (
    /github|gitlab|bitbucket|npm|pypi|docker|vercel|netlify|jetbrains|sentry|huggingface|linear|supabase/.test(
      combined
    )
  ) {
    return '开发平台'
  }
  if (
    /aws|amazon|cloudflare|aliyun|阿里云|tencent|腾讯云|azure|google cloud|gcp|digitalocean|linode|vultr|oracle/.test(
      combined
    )
  ) {
    return '云服务'
  }
  if (
    /google|workspace|gmail|microsoft|office|apple|feishu|飞书|lark|dingtalk|钉钉|slack|notion|zoom|figma/.test(
      combined
    )
  ) {
    return '办公套件'
  }
  if (
    /okta|1password|bitwarden|jumpserver|tailscale|zerotrust|vpn|proxmox|synology|nas|unifi|grafana/.test(
      combined
    )
  ) {
    return '基础设施'
  }
  return '开发平台'
}

/**
 * Clean and validate a Base32 secret string (A-Z, 2-7)
 */
export function sanitizeBase32Secret(raw: string): string {
  return raw.toUpperCase().replace(/[\s\-=]+/g, '').replace(/[^A-Z2-7]/g, '')
}

/**
 * Parse an `otpauth://` URI or raw Base32 secret text into structured 2FA fields
 */
export function parseOtpPayload(rawInput: string): ParsedOtpPayload {
  const trimmed = rawInput.trim()
  if (!trimmed) {
    return {
      ok: false,
      isComplete: false,
      issuer: '',
      account: '',
      secret: '',
      digits: 6,
      period: 30,
      category: '开发平台',
      rawUri: '',
      error: '识别内容为空'
    }
  }

  // Case 0: Multi-line batch input (e.g. multiple otpauth:// or otpauth-migration:// lines)
  const lines = trimmed
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => /^otpauth(-migration)?:\/\//i.test(l))
  if (lines.length > 1) {
    const mergedBatch: ParsedOtpAccountItem[] = []
    for (const line of lines) {
      const sub = parseOtpPayload(line)
      if (sub.ok) {
        if (sub.batchItems && sub.batchItems.length > 0) {
          mergedBatch.push(...sub.batchItems)
        } else if (sub.secret) {
          mergedBatch.push({
            issuer: sub.issuer || '未命名平台',
            account: sub.account || 'default@account',
            secret: sub.secret,
            digits: sub.digits,
            period: sub.period,
            category: sub.category
          })
        }
      }
    }
    if (mergedBatch.length > 0) {
      const first = mergedBatch[0]!
      return {
        ok: true,
        isComplete: true,
        issuer: first.issuer,
        account: first.account,
        secret: first.secret,
        digits: first.digits,
        period: first.period,
        category: first.category,
        rawUri: trimmed,
        batchItems: mergedBatch
      }
    }
  }

  // Case 0.5: Google Authenticator Export / Transfer QR (`otpauth-migration://offline?data=...`)
  if (/^otpauth-migration:\/\//i.test(trimmed)) {
    return parseGoogleMigrationUri(trimmed)
  }

  // Also check if `otpauth-migration://` or `otpauth://` is embedded inside surrounding text
  const embeddedMigration = trimmed.match(/(otpauth-migration:\/\/[^\s"'<>]+)/i)
  if (embeddedMigration?.[1]) {
    return parseGoogleMigrationUri(embeddedMigration[1])
  }

  // Case 1: Standard otpauth:// URI
  if (/^otpauth:\/\//i.test(trimmed)) {
    try {
      const url = new URL(trimmed)
      const rawPath = decodeURIComponent(
        url.pathname.replace(/^\/\/(totp|hotp)\/|^\/(totp|hotp)\//i, '').replace(/^\/+/, '')
      )
      const secretParam = url.searchParams.get('secret') || ''
      const issuerParam = (url.searchParams.get('issuer') || '').trim()
      const digitsParam = parseInt(url.searchParams.get('digits') || '6', 10)
      const periodParam = parseInt(url.searchParams.get('period') || '30', 10)

      let issuer = issuerParam
      let account = ''

      if (rawPath.includes(':')) {
        const colonIdx = rawPath.indexOf(':')
        const prefixIssuer = rawPath.slice(0, colonIdx).trim()
        const restAccount = rawPath.slice(colonIdx + 1).trim()
        issuer = issuerParam || prefixIssuer
        account = restAccount
      } else {
        account = rawPath.trim()
      }

      // If issuer is still empty but account looks like user@domain.com, infer issuer from domain
      if (!issuer && account.includes('@')) {
        const domainPart = account.split('@')[1]?.split('.')[0] || ''
        if (domainPart) {
          issuer = domainPart.charAt(0).toUpperCase() + domainPart.slice(1)
        }
      }

      // If account is empty, use a sensible default
      if (!account && issuer) {
        account = 'default@account'
      }

      const cleanedSecret = sanitizeBase32Secret(secretParam)
      if (!cleanedSecret || cleanedSecret.length < 6) {
        return {
          ok: false,
          isComplete: false,
          issuer,
          account,
          secret: cleanedSecret,
          digits: 6,
          period: 30,
          category: inferCategoryFromIssuer(issuer, account),
          rawUri: trimmed,
          error: '二维码中的 otpauth 链接缺少有效的 Base32 密钥 (secret)'
        }
      }

      const finalIssuer = issuer || account || '未命名平台'
      const category = inferCategoryFromIssuer(finalIssuer, account)

      return {
        ok: true,
        isComplete: Boolean(finalIssuer && cleanedSecret),
        issuer: finalIssuer,
        account: account || 'default@account',
        secret: cleanedSecret,
        digits: Number.isFinite(digitsParam) && digitsParam >= 6 && digitsParam <= 8 ? digitsParam : 6,
        period: Number.isFinite(periodParam) && periodParam > 0 ? periodParam : 30,
        category,
        rawUri: trimmed
      }
    } catch {
      return {
        ok: false,
        isComplete: false,
        issuer: '',
        account: '',
        secret: '',
        digits: 6,
        period: 30,
        category: '开发平台',
        rawUri: trimmed,
        error: 'otpauth:// 链接格式解析失败'
      }
    }
  }

  // Case 2: Raw Base32 secret string (e.g. copied or encoded directly in a plain QR code)
  const compact = trimmed.replace(/[\s\-]+/g, '')
  if (/^[A-Za-z2-7]+=*$/.test(compact) && compact.replace(/=+$/, '').length >= 12) {
    const cleanedSecret = sanitizeBase32Secret(compact)
    return {
      ok: true,
      isComplete: false, // Missing issuer/account, so open form with secret pre-filled
      issuer: '',
      account: '',
      secret: cleanedSecret,
      digits: 6,
      period: 30,
      category: '开发平台',
      rawUri: trimmed
    }
  }

  return {
    ok: false,
    isComplete: false,
    issuer: '',
    account: '',
    secret: '',
    digits: 6,
    period: 30,
    category: '开发平台',
    rawUri: trimmed,
    error: `识别到的二维码不是 2FA 密钥格式 (${trimmed.slice(0, 42)}${trimmed.length > 42 ? '...' : ''})`
  }
}

function loadImageElement(source: string | Blob): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image()
    let objectUrl: string | null = null
    img.onload = () => {
      if (objectUrl) URL.revokeObjectURL(objectUrl)
      resolve(img)
    }
    img.onerror = () => {
      if (objectUrl) URL.revokeObjectURL(objectUrl)
      reject(new Error('无法加载图片数据'))
    }
    if (typeof source === 'string') {
      img.src = source
    } else {
      objectUrl = URL.createObjectURL(source)
      img.src = objectUrl
    }
  })
}

/**
 * Multi-pass high-tolerance QR Code decoder:
 * 0. Uses native macOS CoreImage (`CIDetectorTypeQRCode`) hardware-accelerated decoder first when available
 * 1. Adds a synthetic White Quiet Zone border (28px) around tightly cropped user selections
 * 2. Tries multiple scales (1.0x, 1.6x, 0.65x, 2.2x)
 * 3. Tries high-contrast binarization pass if standard pass fails
 */
export async function decodeQrFromImageSource(source: string | Blob): Promise<string | null> {
  if (isTauriEnv() && typeof source === 'string' && source.startsWith('data:image/')) {
    const nativeHit = await decodeQrFromDataUrlNative(source)
    if (nativeHit && nativeHit.trim()) {
      return nativeHit.trim()
    }
  }

  const img = await loadImageElement(source)
  const srcW = img.naturalWidth || img.width
  const srcH = img.naturalHeight || img.height
  if (!srcW || !srcH) return null

  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d', { willReadFrequently: true })
  if (!ctx) return null

  // If input was a File/Blob in Tauri, also try native macOS CIDetector via canvas PNG export
  if (isTauriEnv() && typeof source !== 'string') {
    canvas.width = srcW
    canvas.height = srcH
    ctx.drawImage(img, 0, 0, srcW, srcH)
    try {
      const dataUrl = canvas.toDataURL('image/png')
      const nativeHit = await decodeQrFromDataUrlNative(dataUrl)
      if (nativeHit && nativeHit.trim()) {
        return nativeHit.trim()
      }
    } catch {
      // Fall through to jsQR
    }
  }

  const scales = [1.0, 1.6, 0.65, 2.2]
  const quietZone = 28

  for (const scale of scales) {
    const drawW = Math.max(40, Math.round(srcW * scale))
    const drawH = Math.max(40, Math.round(srcH * scale))
    if (drawW > 2600 || drawH > 2600) continue

    const totalW = drawW + quietZone * 2
    const totalH = drawH + quietZone * 2
    canvas.width = totalW
    canvas.height = totalH

    // Pass A: White Quiet Zone Padding (fixes tight user crosshair cropping)
    ctx.fillStyle = '#FFFFFF'
    ctx.fillRect(0, 0, totalW, totalH)
    ctx.imageSmoothingEnabled = scale < 1.0
    ctx.drawImage(img, quietZone, quietZone, drawW, drawH)

    const imgData = ctx.getImageData(0, 0, totalW, totalH)
    const hit = jsQR(imgData.data, totalW, totalH, {
      inversionAttempts: 'attemptBoth'
    })
    if (hit && hit.data && hit.data.trim()) {
      return hit.data.trim()
    }

    // Pass B: Multi-threshold high-contrast grayscale binarization pass for dense Google Migration QR codes & dark mode screenshots
    const thresholds = [145, 110, 175]
    for (const thresh of thresholds) {
      const copyData = new Uint8ClampedArray(imgData.data)
      for (let i = 0; i < copyData.length; i += 4) {
        const r = copyData[i]!
        const g = copyData[i + 1]!
        const b = copyData[i + 2]!
        const lum = 0.299 * r + 0.587 * g + 0.114 * b
        const bin = lum < thresh ? 0 : 255
        copyData[i] = bin
        copyData[i + 1] = bin
        copyData[i + 2] = bin
        copyData[i + 3] = 255
      }
      const binHit = jsQR(copyData, totalW, totalH, {
        inversionAttempts: 'attemptBoth'
      })
      if (binHit && binHit.data && binHit.data.trim()) {
        return binHit.data.trim()
      }
    }
  }

  // Pass C: Center-crop magnification pass for full-screen mobile screenshots (e.g. iPhone/Android Google Authenticator export screen where QR sits in the middle)
  if (srcH > srcW * 1.35 && srcW >= 300) {
    const cropRegions = [
      // Middle square region
      { sx: Math.round(srcW * 0.04), sy: Math.round((srcH - srcW) * 0.45), sw: Math.round(srcW * 0.92), sh: Math.round(srcW * 0.92) },
      // Upper-middle region
      { sx: Math.round(srcW * 0.04), sy: Math.round(srcH * 0.18), sw: Math.round(srcW * 0.92), sh: Math.round(srcW * 0.92) }
    ]
    for (const reg of cropRegions) {
      const targetSize = 900
      const totalSize = targetSize + quietZone * 2
      canvas.width = totalSize
      canvas.height = totalSize
      ctx.fillStyle = '#FFFFFF'
      ctx.fillRect(0, 0, totalSize, totalSize)
      ctx.imageSmoothingEnabled = false
      ctx.drawImage(img, reg.sx, reg.sy, reg.sw, reg.sh, quietZone, quietZone, targetSize, targetSize)
      const subData = ctx.getImageData(0, 0, totalSize, totalSize)
      const hit = jsQR(subData.data, totalSize, totalSize, {
        inversionAttempts: 'attemptBoth'
      })
      if (hit && hit.data && hit.data.trim()) {
        return hit.data.trim()
      }
    }
  }

  return null
}

/**
 * Build standard otpauth:// URI and high-DPI QR Code DataURL for mobile sync
 */
export async function buildOtpauthQrCode(
  entry: TotpEntry
): Promise<{ uri: string; qrDataUrl: string }> {
  const issuer = entry.issuer.trim() || 'OmniBox'
  const account = entry.account.trim() || 'user'
  const secret = sanitizeBase32Secret(entry.secret)
  const digits = entry.digits || 6
  const period = entry.period || 30

  const label = `${encodeURIComponent(issuer)}:${encodeURIComponent(account)}`
  const params = new URLSearchParams({
    secret,
    issuer,
    algorithm: 'SHA1',
    digits: String(digits),
    period: String(period)
  })
  const uri = `otpauth://totp/${label}?${params.toString()}`

  const qrDataUrl = await QRCode.toDataURL(uri, {
    width: 240,
    margin: 2,
    color: {
      dark: '#0F172A',
      light: '#FFFFFF'
    },
    errorCorrectionLevel: 'M'
  })

  return { uri, qrDataUrl }
}
