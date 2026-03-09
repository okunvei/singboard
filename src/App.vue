<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import Titlebar from '@/components/layout/Titlebar.vue'
import Sidebar from '@/components/layout/Sidebar.vue'
import ToastHost from '@/components/common/ToastHost.vue'
import { useConfigStore } from '@/stores/config'
import { useServiceStore } from '@/stores/service'
import { useProxiesStore } from '@/stores/proxies'
import { detectRuntimeFiles } from '@/bridge/config'
import { getIPFromIpipnet, getIPFromIpsb } from '@/api/geoip'
import {
  getWechatLatency,
  getBilibiliLatency,
  getGithubLatency,
  getCloudflareLatency,
  getYoutubeLatency,
} from '@/api/latency'

const router = useRouter()
const {
  config,
  updateConfig,
  clashApiUrl,
  clashApiSecret,
  setSingleClashApi,
} = useConfigStore()
const { serviceStatus } = useServiceStore()
const { loadProxies, resumePendingTests } = useProxiesStore()

const setupWizardVisible = ref(false)
const setupError = ref('')
const setupSaving = ref(false)
const setupForm = ref({
  workingDir: '',
  clashApiUrl: '',
  clashApiSecret: '',
})

function hasRequiredPaths() {
  return !!(
    config.value.singboxPath.trim()
    && config.value.configPath.trim()
    && config.value.workingDir.trim()
  )
}

function openSetupWizard(partial?: {
  workingDir?: string
}) {
  setupForm.value = {
    workingDir: partial?.workingDir ?? config.value.workingDir ?? '',
    clashApiUrl: clashApiUrl.value || 'http://127.0.0.1:9090',
    clashApiSecret: clashApiSecret.value || '',
  }
  setupError.value = ''
  setupWizardVisible.value = true
}

async function initRuntimePaths() {
  if (hasRequiredPaths()) return

  const allUnset = !config.value.singboxPath && !config.value.configPath && !config.value.workingDir
  if (!allUnset) {
    openSetupWizard()
    return
  }

  try {
    const detected = await detectRuntimeFiles()
    openSetupWizard({
      workingDir: detected.baseDir,
    })
  } catch {
    openSetupWizard()
  }
}

async function saveSetup() {
  const workingDir = setupForm.value.workingDir.trim()
  const apiUrl = setupForm.value.clashApiUrl.trim()
  const apiSecret = setupForm.value.clashApiSecret

  if (!workingDir) {
    setupError.value = '请填写工作目录。'
    return
  }
  if (!apiUrl) {
    setupError.value = '请填写 Clash API 地址。'
    return
  }

  setupSaving.value = true
  setupError.value = ''
  try {
    const detected = await detectRuntimeFiles(workingDir)
    if (!detected.singboxPath || !detected.configPath) {
      setupError.value = '未在该目录及子目录中检测到 sing-box 核心或 config.json，请检查目录后重试。'
      return
    }

    updateConfig({
      workingDir: detected.baseDir,
      singboxPath: detected.singboxPath,
      configPath: detected.configPath,
    })
    setSingleClashApi(apiUrl, apiSecret)
    setupWizardVisible.value = false
  } catch (e: any) {
    setupError.value = e?.message || '自动检测失败，请检查工作目录是否有效。'
  } finally {
    setupSaving.value = false
  }
}

function goToSettings() {
  setupWizardVisible.value = false
  router.push('/settings')
}

const NETWORK_CACHE_KEY = 'singboard-network'

function runNetworkAutoTest() {
  try {
    const saved = sessionStorage.getItem(NETWORK_CACHE_KEY)
    if (saved) {
      const cached = JSON.parse(saved)
      const hasIP = !!(cached?.chinaIP?.ip || cached?.globalIP?.ip)
      const hasLatency = !!(cached?.latency?.wechat || cached?.latency?.cloudflare)
      if (hasIP && hasLatency) return
    }
  } catch {}

  const result: any = {
    chinaIP: { ip: '', location: '', locationMasked: '' },
    globalIP: { ip: '', location: '', locationMasked: '' },
    latency: { wechat: '', bilibili: '', github: '', cloudflare: '', youtube: '' },
  }

  getIPFromIpipnet().then((res) => {
    const loc = res.data.location.filter(Boolean)
    result.chinaIP = {
      ip: res.data.ip,
      location: loc.join(' '),
      locationMasked: loc.length > 0
        ? loc[0] + ' ' + loc.slice(1).map(() => '**').join(' ')
        : '',
    }
    sessionStorage.setItem(NETWORK_CACHE_KEY, JSON.stringify(result))
  }).catch(() => {})

  getIPFromIpsb().then((res) => {
    const loc = [res.country, res.organization].filter(Boolean).join(' ')
    result.globalIP = { ip: res.ip, location: loc, locationMasked: loc }
    sessionStorage.setItem(NETWORK_CACHE_KEY, JSON.stringify(result))
  }).catch(() => {})

  const latencyTests = [
    { fn: getWechatLatency, key: 'wechat' },
    { fn: getBilibiliLatency, key: 'bilibili' },
    { fn: getGithubLatency, key: 'github' },
    { fn: getCloudflareLatency, key: 'cloudflare' },
    { fn: getYoutubeLatency, key: 'youtube' },
  ]
  for (const { fn, key } of latencyTests) {
    fn().then((ms) => {
      result.latency[key] = ms ? ms.toFixed(0) : '超时'
      sessionStorage.setItem(NETWORK_CACHE_KEY, JSON.stringify(result))
    }).catch(() => {})
  }
}

onMounted(async () => {
  initRuntimePaths()
  await loadProxies()
  resumePendingTests()
})

watch(
  () => serviceStatus.value.state,
  (state, oldState) => {
    if (state === 'running' && oldState !== 'running') {
      setTimeout(runNetworkAutoTest, 3000)
    }
  },
  { immediate: true },
)
</script>

<template>
  <div class="flex flex-col h-screen bg-base-100 text-base-content">
    <Titlebar />
    <div class="flex flex-1 overflow-hidden">
      <Sidebar />
      <main class="flex-1 overflow-auto p-4">
        <router-view />
      </main>
    </div>
    <ToastHost />

    <div
      v-if="setupWizardVisible"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
    >
      <div class="w-full max-w-xl rounded-lg bg-base-100 p-5 shadow-xl space-y-4">
        <h2 class="text-lg font-semibold">初始化向导</h2>
        <p class="text-sm text-base-content/70">
          只需填写工作目录，系统会自动扫描该目录及其子目录，识别 sing-box 核心与配置文件。
        </p>

        <div class="text-sm font-medium text-base-content/70">路径配置</div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">工作目录</span></label>
          <input
            v-model="setupForm.workingDir"
            type="text"
            class="input input-sm input-bordered"
            placeholder="C:\\sing-box"
          />
        </div>

        <div class="divider my-1"></div>
        <div class="text-sm font-medium text-base-content/70">Clash API</div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">API 地址</span></label>
          <input
            v-model="setupForm.clashApiUrl"
            type="text"
            class="input input-sm input-bordered"
            placeholder="http://127.0.0.1:9090"
          />
        </div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">密钥 (Secret)</span></label>
          <input
            v-model="setupForm.clashApiSecret"
            type="password"
            class="input input-sm input-bordered"
            placeholder="留空表示无密钥"
          />
        </div>

        <p v-if="setupError" class="text-sm text-error">{{ setupError }}</p>

        <div class="flex justify-end gap-2">
          <button class="btn btn-sm btn-ghost" @click="goToSettings">前往设置页</button>
          <button class="btn btn-sm btn-primary" :class="{ loading: setupSaving }" @click="saveSetup">
            自动检测并继续
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
