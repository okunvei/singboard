<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useServiceStore } from '@/stores/service'

const route = useRoute()
const router = useRouter()
const { serviceStatus } = useServiceStore()

const navItems = [
  { path: '/overview', label: '概览', icon: 'chart' },
  { path: '/proxies', label: '代理', icon: 'proxy' },
  { path: '/connections', label: '连接', icon: 'connection' },
  { path: '/logs', label: '日志', icon: 'log' },
  { path: '/rules', label: '规则', icon: 'rule' },
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
</script>

<template>
  <div class="flex flex-col w-48 bg-base-200 border-r border-base-300 h-full">
    <nav class="flex-1 py-2">
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
          <template v-else-if="item.icon === 'settings'">⚙️</template>
        </span>
        <span>{{ item.label }}</span>
      </button>
    </nav>

    <div class="p-3 border-t border-base-300">
      <div class="flex items-center gap-2 text-xs text-base-content/60">
        <span class="w-2 h-2 rounded-full" :class="statusColor"></span>
        <span>{{ statusText }}</span>
      </div>
    </div>
  </div>
</template>
