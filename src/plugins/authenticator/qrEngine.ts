import jsQR from 'jsqr'
import QRCode from 'qrcode'
import type { TotpEntry } from '@/bridge/backend'

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
  error?: string
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
 * 1. Adds a synthetic White Quiet Zone border (28px) around tightly cropped user selections
 * 2. Tries multiple scales (1.0x, 1.75x, 0.65x, 2.4x)
 * 3. Tries high-contrast binarization pass if standard pass fails
 */
export async function decodeQrFromImageSource(source: string | Blob): Promise<string | null> {
  const img = await loadImageElement(source)
  const srcW = img.naturalWidth || img.width
  const srcH = img.naturalHeight || img.height
  if (!srcW || !srcH) return null

  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d', { willReadFrequently: true })
  if (!ctx) return null

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

    // Pass B: High-contrast grayscale threshold pass for low-contrast / semi-transparent web QR codes
    const data = imgData.data
    for (let i = 0; i < data.length; i += 4) {
      const r = data[i]!
      const g = data[i + 1]!
      const b = data[i + 2]!
      const lum = 0.299 * r + 0.587 * g + 0.114 * b
      const bin = lum < 145 ? 0 : 255
      data[i] = bin
      data[i + 1] = bin
      data[i + 2] = bin
      data[i + 3] = 255
    }
    const binHit = jsQR(data, totalW, totalH, {
      inversionAttempts: 'attemptBoth'
    })
    if (binHit && binHit.data && binHit.data.trim()) {
      return binHit.data.trim()
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
