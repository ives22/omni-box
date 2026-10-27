# OmniBox Design System Master File (v2.1 - AI-Native Minimalist Edition)

> **Reference Benchmark**: `https://uupm.cc/demo/ai-chatbot-platform` (UI/UX Pro Max Product #18: AI/Chatbot Platform — AI-Native UI + Minimalism & Swiss Style)

---

## 1. Visual Direction & Philosophy
- **Style**: **AI-Native UI + Minimalism & Swiss Style**
- **Core Principles**:
  - **Minimal Chrome & Complete Geometry**: Full 4-corner squircle radius (`rounded-[18px]`) with a 6px breathing margin inside the transparent Tauri window so top and bottom curves and borders are always crisp and never clipped.
  - **Zero Fluorescent Green**: Eliminate harsh `emerald` accents. Use refined **Neutral Slate/Zinc** surfaces paired with restrained **AI Indigo (`#4F46E5` / `#6366F1`)** and **Deep Charcoal (`#0F172A`)** interactive elements.
  - **Tactile Micro-Elevation**: Clean white cards (`#FFFFFF`) on soft cool-gray canvas (`#F8FAFC`) in Light Mode; deep obsidian surfaces (`#0B0F19` / `#111827`) with subtle `1px` hairlines in Dark Mode.

---

## 2. Color Palette
| Token | Light Mode (Swiss Minimal) | Dark Mode (Obsidian AI) | Usage |
| :--- | :--- | :--- | :--- |
| `--color-background` | `#F8FAFC` | `#0B0F19` | Window canvas background |
| `--color-card` | `#FFFFFF` | `#111827` | Elevated cards & panels |
| `--color-muted` | `#F1F5F9` | `#1E293B` | Secondary pills, subtle fills |
| `--color-foreground` | `#0F172A` | `#F8FAFC` | Primary typography |
| `--color-muted-foreground` | `#64748B` | `#94A3B8` | Secondary labels & metadata |
| `--color-accent` | `#4F46E5` (Indigo 600) | `#6366F1` (Indigo 500) | Primary interactive accent |
| `--color-border` | `#E2E8F0` | `#1E293B` | Crisp 1px structural hairlines |

---

## 3. Button & Control System
- **Primary Button (`.btn-primary`)**:
  - Light/Dark: `bg-indigo-600 hover:bg-indigo-500 active:bg-indigo-700 text-white font-medium shadow-sm shadow-indigo-500/15 rounded-xl px-3.5 py-1.5 text-xs transition-all duration-150`
- **Dark Neutral Action Button (`.btn-neutral`)**:
  - Light: `bg-slate-900 hover:bg-slate-800 text-white rounded-xl px-3.5 py-1.5 text-xs font-medium shadow-sm`
  - Dark: `bg-slate-100 hover:bg-white text-slate-900 rounded-xl px-3.5 py-1.5 text-xs font-medium shadow-sm`
- **Secondary / Outline Button (`.btn-secondary`)**:
  - Light: `bg-white hover:bg-slate-50 text-slate-700 border border-slate-200/90 shadow-2xs rounded-xl px-3 py-1.5 text-xs font-medium`
  - Dark: `bg-slate-800/80 hover:bg-slate-700/80 text-slate-200 border border-slate-700/80 rounded-xl px-3 py-1.5 text-xs font-medium`
- **Status Toggle Pill (`.status-pill`)**:
  - Soft capsule with a `6px` status dot indicator (`bg-indigo-500` when active, `bg-slate-400` when muted).
