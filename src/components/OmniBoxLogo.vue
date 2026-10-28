<script setup lang="ts">
withDefaults(
  defineProps<{
    size?: number
    rounded?: boolean
  }>(),
  {
    size: 36,
    rounded: true
  }
)
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 1024 1024"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    class="shrink-0 select-none"
    aria-hidden="true"
  >
    <defs>
      <linearGradient id="omniHyperBg" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#161D38" />
        <stop offset="45%" stop-color="#0D1224" />
        <stop offset="100%" stop-color="#1E153A" />
      </linearGradient>
      <radialGradient id="omniHyperGlow" cx="50%" cy="42%" r="55%">
        <stop offset="0%" stop-color="#6366F1" stop-opacity="0.48" />
        <stop offset="55%" stop-color="#06B6D4" stop-opacity="0.16" />
        <stop offset="100%" stop-color="#000000" stop-opacity="0" />
      </radialGradient>
      <linearGradient id="omniHyperRim" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#A5B4FC" stop-opacity="0.7" />
        <stop offset="50%" stop-color="#6366F1" stop-opacity="0.25" />
        <stop offset="100%" stop-color="#C084FC" stop-opacity="0.5" />
      </linearGradient>
      <linearGradient id="omniTopFace" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#67E8F9" />
        <stop offset="50%" stop-color="#60A5FA" />
        <stop offset="100%" stop-color="#818CF8" />
      </linearGradient>
      <linearGradient id="omniLeftFace" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#4F46E5" />
        <stop offset="100%" stop-color="#1E1B4B" />
      </linearGradient>
      <linearGradient id="omniRightFace" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#7C3AED" />
        <stop offset="100%" stop-color="#3B0764" />
      </linearGradient>
    </defs>

    <!-- Full-Bleed or Squircle Base -->
    <rect
      x="32"
      y="32"
      width="960"
      height="960"
      :rx="rounded ? 225 : 480"
      fill="url(#omniHyperBg)"
    />
    <rect
      x="32"
      y="32"
      width="960"
      height="960"
      :rx="rounded ? 225 : 480"
      fill="url(#omniHyperGlow)"
    />
    <rect
      x="40"
      y="40"
      width="944"
      height="944"
      :rx="rounded ? 218 : 472"
      fill="none"
      stroke="url(#omniHyperRim)"
      stroke-width="14"
    />

    <!-- Isometric 3D HyperCube -->
    <g>
      <!-- Outer Hexagon Halo -->
      <polygon
        points="512,185 792,346 792,668 512,829 232,668 232,346"
        fill="none"
        stroke="#818CF8"
        stroke-width="8"
        stroke-opacity="0.38"
        stroke-linejoin="round"
      />

      <!-- Top Face -->
      <polygon points="512,198 776,350 512,502 248,350" fill="url(#omniTopFace)" />
      <polygon
        points="512,254 680,350 512,446 344,350"
        fill="#0F172A"
        fill-opacity="0.74"
        stroke="#BAE6FD"
        stroke-width="6"
        stroke-opacity="0.65"
      />
      <circle cx="512" cy="350" r="36" fill="#38BDF8" />

      <!-- Left Face -->
      <polygon points="248,350 512,502 512,808 248,656" fill="url(#omniLeftFace)" />
      <!-- Right Face -->
      <polygon points="512,502 776,350 776,656 512,808" fill="url(#omniRightFace)" />

      <!-- Glowing Inner Ridge Edges -->
      <polyline
        points="248,350 512,502 776,350"
        fill="none"
        stroke="#E0F2FE"
        stroke-width="10"
        stroke-linecap="round"
        stroke-linejoin="round"
        opacity="0.92"
      />
      <line
        x1="512"
        y1="502"
        x2="512"
        y2="808"
        stroke="#C7D2FE"
        stroke-width="10"
        stroke-linecap="round"
        opacity="0.88"
      />
      <polygon
        points="512,198 776,350 776,656 512,808 248,656 248,350"
        fill="none"
        stroke="#A5B4FC"
        stroke-width="7"
        stroke-opacity="0.55"
        stroke-linejoin="round"
      />

      <!-- Left Face: Terminal Prompt >_ -->
      <path
        d="M 322 490 L 388 564 L 322 598"
        fill="none"
        stroke="#38BDF8"
        stroke-width="26"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <line
        x1="408"
        y1="626"
        x2="466"
        y2="658"
        stroke="#F8FAFC"
        stroke-width="26"
        stroke-linecap="round"
      />

      <!-- Right Face: Modular Bento Grid Tiles -->
      <g opacity="0.95">
        <polygon
          points="564,532 638,488 638,562 564,606"
          fill="#C084FC"
          fill-opacity="0.4"
          stroke="#E9D5FF"
          stroke-width="6"
        />
        <polygon
          points="660,476 734,432 734,506 660,550"
          fill="#38BDF8"
          fill-opacity="0.5"
          stroke="#BAE6FD"
          stroke-width="6"
        />
        <polygon
          points="564,630 638,586 638,660 564,704"
          fill="#A855F7"
          fill-opacity="0.35"
          stroke="#D8B4FE"
          stroke-width="6"
        />
        <polygon
          points="660,574 734,530 734,604 660,648"
          fill="#818CF8"
          fill-opacity="0.4"
          stroke="#C7D2FE"
          stroke-width="6"
        />
      </g>
    </g>

    <!-- Top Right AI Sparkle -->
    <path
      d="M 786 185 Q 786 242 843 242 Q 786 242 786 299 Q 786 242 729 242 Q 786 242 786 185 Z"
      fill="#67E8F9"
    />
  </svg>
</template>
