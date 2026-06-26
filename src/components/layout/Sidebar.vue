<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useServiceStore } from '@/stores/service'
import { useConfigStore } from '@/stores/config'
import { normalizeVersionText } from '@/utils/format'
import { useToastStore } from '@/stores/toast'
import { getSingboxVersion, validateSingboxConfig, getRunningConfigPath, getRemoteConfigPath, copyToRunningConfig } from '@/bridge/config'
import { startService, stopService } from '@/bridge/service'
import { open } from '@tauri-apps/plugin-shell' // ✨ 新加：用来调用系统浏览器
import { invoke } from '@tauri-apps/api/core'

const route = useRoute()
const router = useRouter()
const { serviceStatus, statusText, refresh } = useServiceStore()
const { config, configProfiles, clashApiUrl, clashApiSecret } = useConfigStore()

// ✨ 是否正在运行，用于控制【访问 WebUI】按钮可点击状态
const isRunning = computed(() => serviceStatus.value.state === 'running')

// ✨ 新加：拼接地址并打开系统浏览器访问 WebUI
const openWebUI = async () => {
  // ✨ 逻辑拦截：如果没运行，直接返回
  if (!isRunning.value) return

  try {
    // 1. 直接使用 store 提供的当前激活地址和密钥
    const apiUrl = clashApiUrl.value || 'http://127.0.0.1:9090'
    const secret = clashApiSecret.value || ''

    // 2. 解析 URL（提取域名和端口）
    const url = new URL(apiUrl)
    const hostname = url.hostname
    const port = url.port || (url.protocol === 'https:' ? '443' : '80')

    // 3. 拼接地址：基础地址
    let targetUrl = `${apiUrl}/ui/?hostname=${hostname}&port=${port}`

    // 4. 加上 secret
    if (secret) {
      targetUrl += `&secret=${secret}`
    }

    // 5. 召唤系统默认浏览器
    await open(targetUrl)
  } catch (error) {
    console.error('无法解析地址，请检查配置是否正确:', error)
  }
}
const { pushToast } = useToastStore()
const singboxVersion = ref('')
const actionLoading = ref('') 
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
  { path: '/config', label: '配置', icon: 'config' },
  { path: '/settings', label: '设置', icon: 'settings' },
]

const currentPath = computed(() => route.path)

function navigate(path: string) { router.push(path) }

async function refreshVersion() {
  // 注意：删除了之前的 "if (state !== 'running') return" 判断
  const singboxPath = config.value.singboxPath?.trim()
  if (!singboxPath) {
    singboxVersion.value = '未配置内核路径'
    return
  }
  try {
    const raw = await getSingboxVersion(singboxPath)
    singboxVersion.value = normalizeVersionText(raw)
  } catch (e: any) {
    const errMsg = String(e?.message || e)
    // 捕获“找不到文件”的错误
    if (errMsg.includes('not found') || errMsg.includes('2')) {
      singboxVersion.value = '❌ 未找到内核文件'
    } else {
      singboxVersion.value = '版本获取失败'
    }
  }
}

// 重启或停止前检测系统代理是否开启，若开启则清除
async function tryCheckAndClearProxy() {
  try {
    const isOn = await invoke<boolean>('check_system_proxy_inbound', { configPath: '' })
    if (isOn) {
      await invoke('clear_macos_system_proxy')
    }
  } catch {
    // 检测失败不影响停止流程
  }
}

// 按钮控制逻辑
async function validateBeforeStart(): Promise<boolean> {
  const { singboxPath, workingDir } = config.value
  if (!singboxPath) {
    pushToast({ message: '请先在设置中配置内核路径', type: 'error' })
    return false
  }
  const activeId = config.value.activeConfigProfileId
  const profile = configProfiles.value.find((p) => p.id === activeId)

  try {
    let configPath = ''
    if (profile) {
      configPath = profile.type === 'local' ? profile.source : await getRemoteConfigPath(profile.id)
      await validateSingboxConfig(singboxPath, configPath, workingDir)
      await copyToRunningConfig(configPath)
    } else {
      configPath = await getRunningConfigPath()
      await validateSingboxConfig(singboxPath, configPath, workingDir)
    }
    return true
  } catch (e: any) {
    pushToast({ message: '校验失败: ' + (e?.message || e), type: 'error' })
    return false
  }
}

async function handleServiceAction(action: 'start' | 'stop' | 'restart') {
  actionLoading.value = action
  try {
    const name = config.value.serviceName
    if (action === 'start' || action === 'restart') {
      if (!(await validateBeforeStart())) return
      if (action === 'restart') await stopService(name)
      if (action === 'restart') {
        await tryCheckAndClearProxy()
        await stopService(name)
      }
      await startService(name)
    } else {
      await tryCheckAndClearProxy()
      await stopService(name)
    }
    setTimeout(refresh, 1000)
  } catch (e: any) {
    pushToast({ message: '操作失败: ' + e, type: 'error' })
  } finally {
    setTimeout(() => { actionLoading.value = '' }, 2000)
  }
}

// 状态颜色
const statusColor = computed(() => {
  switch (serviceStatus.value.state) {
    case 'running': return 'bg-success shadow-[0_0_5px_rgba(34,197,94,0.4)]'
    case 'stopped': return 'bg-error shadow-[0_0_5px_rgba(239,68,68,0.4)]'
    case 'starting':
    case 'stopping': return 'bg-warning animate-pulse' // 停止时黄色警告颜色
    case 'not_installed': return 'bg-error opacity-50' // 未安装时红颜色突出表示重要提示
    default: return 'bg-base-content/30'
  }
})

// 监听与尺寸测量

function measureOverflow() {
  const wrap = versionWrapEl.value
  const track = versionTrackEl.value
  if (!wrap || !track || !singboxVersion.value) {
    shouldScrollVersion.value = false; versionOverflowPx.value = 0; return
  }
  const overflow = track.offsetWidth - wrap.clientWidth
  shouldScrollVersion.value = overflow > 2
  versionOverflowPx.value = Math.max(0, Math.ceil(overflow))
}

watch(() => [serviceStatus.value.state, config.value.singboxPath], () => { void refreshVersion() }, { immediate: true })
watch(versionWrapEl, (el) => {
  resizeOb?.disconnect()
  if (el) {
    resizeOb = new ResizeObserver(() => measureOverflow())
    resizeOb.observe(el)
  }
})

watch(singboxVersion, () => { requestAnimationFrame(() => measureOverflow()) })
onBeforeUnmount(() => { resizeOb?.disconnect(); resizeOb = null })
</script>

<template>
  <div class="flex flex-col w-48 bg-base-200 border-r border-base-300 h-full">
    <nav class="flex-1 py-2 overflow-y-auto">
      <button
        v-for="item in navItems"
        :key="item.path"
        class="w-full flex items-center gap-3 px-4 py-2.5 text-sm transition-colors"
        :class="currentPath === item.path ? 'bg-primary/10 text-primary font-medium border-r-2 border-primary' : 'hover:bg-base-300 text-base-content/70'"
        @click="navigate(item.path)"
      >
        <span class="w-5 text-center emoji-font">
          <template v-if="item.icon === 'chart'">📊</template>
          <template v-else-if="item.icon === 'proxy'">🔀</template>
          <template v-else-if="item.icon === 'connection'">🔗</template>
          <template v-else-if="item.icon === 'log'">📝</template>
          <template v-else-if="item.icon === 'rule'">📋</template>
          <template v-else-if="item.icon === 'config'">📄</template>
          <template v-else-if="item.icon === 'settings'">⚙️</template>
        </span>
        <span>{{ item.label }}</span>
      </button>
    </nav>

    <div class="mb-0.5 px-4 pt-2.5 pb-2 border-t border-base-300 flex justify-center">
      <button
        @click="openWebUI"
        :disabled="!isRunning"
        class="btn btn-sm transition-all"
        :class="isRunning ? 'btn-primary' : 'btn-ghost bg-base-300 text-base-content/30 cursor-not-allowed'"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="mr-1">
          <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
          <polyline points="15 3 21 3 21 9"></polyline>
          <line x1="10" y1="14" x2="21" y2="3"></line>
        </svg>
        访问 WebUI
      </button>
    </div>

    <div class="px-4 py-1.5 pt-2.5 border-t border-base-300 space-y-3">
      <div class="flex flex-col gap-1.5 px-2">
        <span class="text-sm font-bold opacity-70">核心控制</span>
        <div class="flex items-center gap-2 text-xs text-base-content/80">
          <span class="w-2 h-2 rounded-full shrink-0" :class="statusColor"></span>
          <span>{{ statusText }}</span>
        </div>
      </div>
      
      <div class="flex flex-col gap-1">
        <button 
          class="btn btn-sm btn-ghost w-full justify-start font-normal hover:bg-success/10 hover:text-success text-base-content/80" 
          :disabled="serviceStatus.state === 'running' || serviceStatus.state === 'not_installed' || !!actionLoading"
          @click="handleServiceAction('start')"
        >
          <span v-if="actionLoading === 'start'" class="loading loading-spinner loading-xs"></span>
          🚀 启动核心
        </button>

        <button 
          class="btn btn-sm btn-ghost w-full justify-start font-normal hover:bg-warning/10 hover:text-warning text-base-content/80"
          :disabled="serviceStatus.state === 'not_installed' || !!actionLoading"
          @click="handleServiceAction('restart')"
        >
          <span v-if="actionLoading === 'restart'" class="loading loading-spinner loading-xs"></span>
          🔄 重启核心
        </button>

        <button 
          class="btn btn-sm btn-ghost w-full justify-start font-normal hover:bg-error/10 hover:text-error text-base-content/80"
          :disabled="serviceStatus.state === 'stopped' || serviceStatus.state === 'not_installed' || !!actionLoading"
          @click="handleServiceAction('stop')"
        >
          <span v-if="actionLoading === 'stop'" class="loading loading-spinner loading-xs"></span>
          🛑 停止核心
        </button>
      </div>
    </div>

    <div class="px-4 py-3 border-t border-base-300 bg-base-300/30">
      <div class="flex items-center gap-2 text-xs text-base-content/60 whitespace-nowrap overflow-hidden">
        <span
          ref="versionWrapEl"
          class="version-wrap text-base-content/60"
          :class="{ scrolling: shouldScrollVersion }"
          :style="{ '--overflow-distance': versionOverflowPx }"
          :title="singboxVersion"
        >
          <span ref="versionTrackEl" class="version-track">
            <span class="version-item">{{ singboxVersion || '正在检测核心...' }}</span>
          </span>
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.version-wrap { flex: 1; min-width: 0; overflow: hidden; white-space: nowrap; }
.version-track { display: inline-flex; align-items: center; }
.version-wrap.scrolling .version-track { animation: version-pingpong 4.5s ease-in-out infinite alternate; will-change: transform; }
.version-item { flex: 0 0 auto; }
@keyframes version-pingpong { 0% { transform: translateX(0); } 100% { transform: translateX(calc(-1px * var(--overflow-distance, 0))); } }
</style>
