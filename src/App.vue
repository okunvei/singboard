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
const setupForm = ref({
  singboxPath: '',
  configPath: '',
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
  singboxPath?: string
  configPath?: string
  workingDir?: string
}) {
  setupForm.value = {
    singboxPath: partial?.singboxPath ?? config.value.singboxPath ?? '',
    configPath: partial?.configPath ?? config.value.configPath ?? '',
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
      singboxPath: detected.singboxPath,
      configPath: detected.configPath,
      workingDir: detected.baseDir,
    })
  } catch {
    openSetupWizard()
  }
}

function saveSetup() {
  const singboxPath = setupForm.value.singboxPath.trim()
  const configPath = setupForm.value.configPath.trim()
  const workingDir = setupForm.value.workingDir.trim()
  const clashApiUrl = setupForm.value.clashApiUrl.trim()
  const clashApiSecret = setupForm.value.clashApiSecret

  if (!singboxPath || !configPath || !workingDir) {
    setupError.value = '请填写核心路径、配置文件路径和工作目录。'
    return
  }
  if (!clashApiUrl) {
    setupError.value = '请填写 Clash API 地址。'
    return
  }

  updateConfig({ singboxPath, configPath, workingDir })
  setSingleClashApi(clashApiUrl, clashApiSecret)
  setupWizardVisible.value = false
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
          首次运行请手动确认路径配置和 Clash API 设置。
        </p>

        <div class="text-sm font-medium text-base-content/70">路径配置</div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">sing-box 核心路径</span></label>
          <input
            v-model="setupForm.singboxPath"
            type="text"
            class="input input-sm input-bordered"
            placeholder="C:\\sing-box\\sing-box.exe"
          />
        </div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">配置文件路径</span></label>
          <input
            v-model="setupForm.configPath"
            type="text"
            class="input input-sm input-bordered"
            placeholder="C:\\sing-box\\config.json"
          />
        </div>
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
          <button class="btn btn-sm btn-primary" @click="saveSetup">保存并继续</button>
        </div>
      </div>
    </div>
  </div>
</template>
