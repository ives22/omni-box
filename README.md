# OmniBox

基于 **Tauri v2 (Rust) + Vue 3.5 + TypeScript + Tailwind CSS + SQLite** 构建的桌面命令启动器与微应用工作台（macOS 原生多屏跟随、非激活全屏悬浮面板 `NSPanel`、独立多窗口与 Frecency 智能排序）。

## 核心特性

- **原生级搜索与多显示器体验**
  - 基于 Rust 内存索引、拼音全拼/首字母匹配 (`pinyin`) 与 SQLite Frecency 频次权重算法，亚毫秒级响应。
  - macOS 自定义 `OmniBoxFullscreenPanel`（`NSNonactivatingPanelMask` + `NSWindowCollectionBehaviorFullScreenAuxiliary`），支持在任意全屏应用上方无缝呼出，鼠标在哪块屏幕就在哪块屏幕弹出。
  - 支持 `Command + D` / 右键菜单将任意工具插件**分离为真正的独立原生窗口**，与主搜索框互不干扰。
  - 支持双击顶栏/空白处基于当前屏幕自适应比例（`86% × 84%`）准最大化与原位还原。

- **内置高频开发者微应用**
  1. **JSON 助手 (`JsonStudio`)**：双栏实时同步编辑、树形层级折叠/展开、JSON / XML / YAML / 转义 / TS·Rust·Go 结构体互转、Key 升序/降序排列、`2/4/Tab` 缩进切换，以及支持虚拟滚动与 `v-memo` 的 `Ctrl/Cmd + F` 高性能搜索与匹配行过滤。
  2. **2FA 验证码 (`Authenticator`)**：RFC 6238 标准 TOTP 动态令牌实时计算，支持**屏幕十字框选扫码自动识别入库**（`Cmd + Shift + S`）、`Cmd + V` 剪贴板截图/链接智能粘贴、拖拽本地二维码图片识别，以及反向生成同步二维码供手机扫码备份。
  3. **计算稿纸 (`CalcPad`)**：多行实时表达式计算、变量声明引用、单位换算与自动聚合求和。
  4. **文本对比 (`DiffViewer`)**：左右双栏与行内字符级代码/文本差异对比。
  5. **密码保险箱 (`PasswordVault`)**：基于 `Argon2id` 密钥派生 + `AES-256-GCM` 本地加密存储。
  6. **插件市场与 uTools 兼容层 (`UtoolsHub`)**：内置 `window.utools` / `window.rubick` API 兼容适配层，支持导入和运行轻量插件。

## 本地开发与构建

```bash
# 1. 安装前端依赖
pnpm install

# 2. 启动 Tauri 桌面端开发服务
pnpm tauri dev

# 3. 生产环境类型检查与构建
pnpm build
pnpm tauri build
```
