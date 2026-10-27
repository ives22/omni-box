<script setup lang="ts">
import { computed } from 'vue'
import {
  Calculator,
  Braces,
  GitCompare,
  ShieldCheck,
  KeyRound,
  Blocks,
  TerminalSquare,
  Terminal,
  Globe,
  AppWindow,
  Search,
  Sparkles,
  Copy,
  Check,
  Plus,
  Trash2,
  Eye,
  EyeOff,
  RefreshCw,
  Lock,
  Unlock,
  ArrowLeftRight,
  FileCode2,
  Wand2,
  Sun,
  Moon,
  CornerDownLeft,
  X,
  ChevronRight,
  FolderGit2,
  Play,
  SlidersHorizontal,
  Download,
  Upload,
  Minus,
  Maximize2,
  Minimize2,
  Move,
  Pin,
  UserCircle,
  Database,
  Settings,
  Rocket,
  CornerUpRight,
  Cpu,
  LayoutGrid,
  Plug,
  HelpCircle,
  ScanLine,
  QrCode,
  ImageUp,
  ClipboardPaste,
  Bookmark,
  Pencil,
  Undo2,
  ChevronUp,
  ChevronDown,
  ChevronsUpDown,
  WrapText,
  Table2,
  Cloud,
  Server,
  ExternalLink,
  CopyPlus,
  FileSpreadsheet,
  AlertTriangle,
  Layers
} from 'lucide-vue-next'

const props = withDefaults(
  defineProps<{
    name: string
    size?: number
    strokeWidth?: number
    className?: string
  }>(),
  {
    size: 18,
    strokeWidth: 1.75,
    className: ''
  }
)

const iconMap: Record<string, unknown> = {
  Calculator,
  Braces,
  GitCompare,
  ShieldCheck,
  KeyRound,
  Blocks,
  TerminalSquare,
  Terminal,
  Globe,
  AppWindow,
  Search,
  Sparkles,
  Copy,
  Check,
  Plus,
  Trash2,
  Eye,
  EyeOff,
  RefreshCw,
  Lock,
  Unlock,
  ArrowLeftRight,
  FileCode2,
  Wand2,
  Sun,
  Moon,
  CornerDownLeft,
  X,
  ChevronRight,
  FolderGit2,
  Play,
  SlidersHorizontal,
  Download,
  Upload,
  Minus,
  Maximize2,
  Minimize2,
  Move,
  Pin,
  UserCircle,
  Database,
  Settings,
  Rocket,
  CornerUpRight,
  Cpu,
  LayoutGrid,
  Plug,
  HelpCircle,
  ScanLine,
  QrCode,
  ImageUp,
  ClipboardPaste,
  Bookmark,
  Pencil,
  Undo2,
  ChevronUp,
  ChevronDown,
  ChevronsUpDown,
  WrapText,
  Table2,
  Cloud,
  Server,
  ExternalLink,
  CopyPlus,
  FileSpreadsheet,
  AlertTriangle,
  Layers
}

const resolvedIcon = computed(() => {
  return iconMap[props.name] || AppWindow
})
</script>

<template>
  <component
    :is="resolvedIcon"
    :size="size"
    :stroke-width="strokeWidth"
    :class="className"
    aria-hidden="true"
  />
</template>
