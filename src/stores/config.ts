import { ref, computed, watch } from 'vue'
import type { AppConfig, ClashApiProfile } from '@/types'

const STORAGE_KEY = 'singboard-config'

function createClashApiId(): string {
  return `api_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
}

function createClashApiProfile(name: string, url: string, secret: string): ClashApiProfile {
  return {
    id: createClashApiId(),
    name,
    url,
    secret,
  }
}

function normalizeConfig(raw: any): AppConfig {
  const normalizeWindowsPath = (value: unknown): string => {
    if (typeof value !== 'string') return ''
    if (value.startsWith('\\\\?\\UNC\\')) return `\\\\${value.slice(8)}`
    if (value.startsWith('\\\\?\\')) return value.slice(4)
    return value
  }

  const savedProfiles = Array.isArray(raw?.clashApis)
    ? raw.clashApis
      .filter((item: any) => item && typeof item === 'object')
      .map((item: any): ClashApiProfile => ({
        id: typeof item.id === 'string' && item.id ? item.id : createClashApiId(),
        name: typeof item.name === 'string' && item.name.trim() ? item.name.trim() : 'API',
        url: typeof item.url === 'string' && item.url.trim() ? item.url.trim() : 'http://127.0.0.1:9090',
        secret: typeof item.secret === 'string' ? item.secret : '',
      }))
    : []

  const legacyUrl = typeof raw?.clashApiUrl === 'string' && raw.clashApiUrl.trim()
    ? raw.clashApiUrl.trim()
    : 'http://127.0.0.1:9090'
  const legacySecret = typeof raw?.clashApiSecret === 'string' ? raw.clashApiSecret : ''

  const clashApis: ClashApiProfile[] = savedProfiles.length > 0
    ? savedProfiles
    : [createClashApiProfile('默认 API', legacyUrl, legacySecret)]

  const activeClashApiId =
    typeof raw?.activeClashApiId === 'string'
    && clashApis.some((api) => api.id === raw.activeClashApiId)
      ? raw.activeClashApiId
      : clashApis[0].id

  return {
    clashApis,
    activeClashApiId,
    singboxPath: normalizeWindowsPath(raw?.singboxPath),
    configPath: normalizeWindowsPath(raw?.configPath),
    workingDir: normalizeWindowsPath(raw?.workingDir),
    serviceName: typeof raw?.serviceName === 'string' && raw.serviceName ? raw.serviceName : 'sing-box',
    theme: typeof raw?.theme === 'string' && raw.theme ? raw.theme : 'light',
    latencyTestUrl: typeof raw?.latencyTestUrl === 'string' && raw.latencyTestUrl
      ? raw.latencyTestUrl
      : 'https://www.gstatic.com/generate_204',
    ipv6TestEnabled: typeof raw?.ipv6TestEnabled === 'boolean' ? raw.ipv6TestEnabled : false,
    groupTestUrls: raw?.groupTestUrls && typeof raw.groupTestUrls === 'object' && !Array.isArray(raw.groupTestUrls)
      ? Object.fromEntries(
          Object.entries(raw.groupTestUrls as Record<string, unknown>).filter((e): e is [string, string] => typeof e[1] === 'string' && e[1].length > 0)
        )
      : {},
  }
}

function loadConfig(): AppConfig {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved) {
      return normalizeConfig(JSON.parse(saved))
    }
  } catch { }
  return normalizeConfig({})
}

const config = ref<AppConfig>(loadConfig())

function applyTheme(theme: string) {
  document.documentElement.setAttribute('data-theme', theme)
}

applyTheme(config.value.theme)

watch(config, (val) => {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(val))
  applyTheme(val.theme)
}, { deep: true })

export function useConfigStore() {
  const clashApis = computed(() => config.value.clashApis)
  const activeClashApiId = computed(() => config.value.activeClashApiId)
  const activeClashApi = computed(() =>
    config.value.clashApis.find((api) => api.id === config.value.activeClashApiId)
    ?? config.value.clashApis[0],
  )
  const clashApiUrl = computed(() => activeClashApi.value?.url ?? '')
  const clashApiSecret = computed(() => activeClashApi.value?.secret ?? '')
  const serviceName = computed(() => config.value.serviceName)

  function updateConfig(partial: Partial<AppConfig>) {
    config.value = normalizeConfig({ ...config.value, ...partial })
  }

  function setActiveClashApi(id: string) {
    if (config.value.clashApis.some((api) => api.id === id)) {
      config.value.activeClashApiId = id
    }
  }

  function addClashApi(name: string, url: string, secret: string): string {
    const profile = createClashApiProfile(name, url, secret)
    config.value.clashApis.push(profile)
    return profile.id
  }

  function updateActiveClashApi(partial: Partial<Omit<ClashApiProfile, 'id'>>) {
    const current = activeClashApi.value
    if (!current) return
    if (typeof partial.name === 'string') current.name = partial.name
    if (typeof partial.url === 'string') current.url = partial.url
    if (typeof partial.secret === 'string') current.secret = partial.secret
  }

  function removeClashApi(id: string): boolean {
    if (config.value.clashApis.length <= 1) return false
    const index = config.value.clashApis.findIndex((api) => api.id === id)
    if (index === -1) return false
    const removingActive = config.value.activeClashApiId === id
    config.value.clashApis.splice(index, 1)
    if (removingActive) {
      config.value.activeClashApiId = config.value.clashApis[0].id
    }
    return true
  }

  function setSingleClashApi(url: string, secret: string, name = '默认 API') {
    const profile = createClashApiProfile(name, url, secret)
    config.value.clashApis = [profile]
    config.value.activeClashApiId = profile.id
  }

  return {
    config,
    clashApis,
    activeClashApi,
    activeClashApiId,
    clashApiUrl,
    clashApiSecret,
    serviceName,
    updateConfig,
    setActiveClashApi,
    addClashApi,
    updateActiveClashApi,
    removeClashApi,
    setSingleClashApi,
  }
}
