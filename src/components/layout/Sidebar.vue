<script setup lang="ts">
import { computed, ref, watch, onMounted, onBeforeUnmount } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useServiceStore } from '@/stores/service'
import { useConfigStore } from '@/stores/config'
import { getSingboxVersion } from '@/bridge/config'

const route = useRoute()
const router = useRouter()
const { serviceStatus } = useServiceStore()
const { config } = useConfigStore()
const singboxVersion = ref('')
const versionWrapEl = ref<HTMLElement | null>(null)
const versionTrackEl = ref<HTMLElement | null>(null)
const shouldScrollVersion = ref(false)
const versionOverflowPx = ref(0)
let resizeOb: ResizeObserver | null = null

const navItems = [
  { path: '/overview', label: '概览', icon: 'chart' },
  { path: '/proxies', label: '代理', icon: 'proxy' },
  { path: '/connections', label: '连接', icon: 'connection' },
  { path: '/logs', label: '日志', icon: 'log' },
  { path: '/rules', label: '规则', icon: 'rule' },
  { path: '/config-editor', label: '配置编辑', icon: 'editor' },
  { path: '/settings', label: '设置', icon: 'settings' },
]

const currentPath = computed(() => route.path)

function navigate(path: string) {
  router.push(path)
}

const statusColor = computed(() => {
  switch (serviceStatus.value.state) {
    case 'running': return 'bg-success'
    case 'stopped': return 'bg-error'
    case 'starting':
    case 'stopping': return 'bg-warning'
    default: return 'bg-base-content/30'
  }
})

const statusText = computed(() => {
  const map: Record<string, string> = {
    running: '运行中',
    stopped: '已停止',
    starting: '启动中',
    stopping: '停止中',
    not_installed: '未安装',
    unknown: '未知',
  }
  return map[serviceStatus.value.state] || '未知'
})

function normalizeVersionText(raw: string): string {
  const firstLine = raw
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => !!line) ?? raw.trim()

  return firstLine.replace(/\bversion\b/ig, '').replace(/\s{2,}/g, ' ').trim()
}

async function refreshVersion() {
  if (serviceStatus.value.state !== 'running') {
    singboxVersion.value = ''
    return
  }
  const singboxPath = config.value.singboxPath?.trim()
  if (!singboxPath) {
    singboxVersion.value = ''
    return
  }
  try {
    const raw = await getSingboxVersion(singboxPath)
    singboxVersion.value = normalizeVersionText(raw)
  } catch {
    singboxVersion.value = ''
  }
}

function measureOverflow() {
  const wrap = versionWrapEl.value
  const track = versionTrackEl.value
  if (!wrap || !track || !singboxVersion.value) {
    shouldScrollVersion.value = false
    versionOverflowPx.value = 0
    return
  }
  const overflow = track.offsetWidth - wrap.clientWidth
  shouldScrollVersion.value = overflow > 2
  versionOverflowPx.value = Math.max(0, Math.ceil(overflow))
}

watch(
  () => [serviceStatus.value.state, config.value.singboxPath],
  () => {
    void refreshVersion()
  },
  { immediate: true },
)

watch(versionWrapEl, (el, oldEl) => {
  resizeOb?.disconnect()
  if (el) {
    resizeOb = new ResizeObserver(() => measureOverflow())
    resizeOb.observe(el)
  }
})

watch(singboxVersion, () => {
  requestAnimationFrame(() => measureOverflow())
})

onBeforeUnmount(() => {
  resizeOb?.disconnect()
  resizeOb = null
})
</script>

<template>
  <div class="flex flex-col w-48 bg-base-200 border-r border-base-300 h-full">
    <nav class="flex-1 py-2 overflow-y-auto">
      <button
        v-for="item in navItems"
        :key="item.path"
        class="w-full flex items-center gap-3 px-4 py-2.5 text-sm transition-colors"
        :class="
          currentPath === item.path
            ? 'bg-primary/10 text-primary font-medium border-r-2 border-primary'
            : 'hover:bg-base-300 text-base-content/70'
        "
        @click="navigate(item.path)"
      >
        <span class="w-5 text-center emoji-font">
          <template v-if="item.icon === 'chart'">📊</template>
          <template v-else-if="item.icon === 'proxy'">🔀</template>
          <template v-else-if="item.icon === 'connection'">🔗</template>
          <template v-else-if="item.icon === 'log'">📝</template>
          <template v-else-if="item.icon === 'rule'">📋</template>
          <template v-else-if="item.icon === 'editor'">🧩</template>
          <template v-else-if="item.icon === 'settings'">⚙️</template>
        </span>
        <span>{{ item.label }}</span>
      </button>
    </nav>

    <div class="p-3 border-t border-base-300">
      <div class="flex items-center gap-2 text-xs text-base-content/60 whitespace-nowrap overflow-hidden">
        <span class="w-2 h-2 rounded-full shrink-0" :class="statusColor"></span>
        <span class="shrink-0">{{ statusText }}</span>
        <span
          v-if="serviceStatus.state === 'running' && singboxVersion"
          ref="versionWrapEl"
          class="version-wrap text-base-content/45"
          :class="{ scrolling: shouldScrollVersion }"
          :style="{ '--overflow-distance': versionOverflowPx }"
          :title="singboxVersion"
        >
          <span ref="versionTrackEl" class="version-track">
            <span class="version-item">{{ singboxVersion }}</span>
          </span>
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.version-wrap {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
}

.version-track {
  display: inline-flex;
  align-items: center;
}

.version-wrap.scrolling .version-track {
  animation: version-pingpong 4.5s ease-in-out infinite alternate;
  will-change: transform;
}

.version-item {
  flex: 0 0 auto;
}

@keyframes version-pingpong {
  0% {
    transform: translateX(0);
  }
  100% {
    transform: translateX(calc(-1px * var(--overflow-distance, 0)));
  }
}
</style>
