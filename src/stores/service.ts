import { ref, onUnmounted } from 'vue'
import { queryServiceStatus } from '@/bridge/service'
import { useConfigStore } from './config'
import type { ServiceStatus } from '@/types'

const serviceStatus = ref<ServiceStatus>({ state: 'unknown' })
let pollTimer: ReturnType<typeof setInterval> | null = null
let refCount = 0

async function poll() {
  const { serviceName } = useConfigStore()
  try {
    serviceStatus.value = await queryServiceStatus(serviceName.value)
  } catch {
    serviceStatus.value = { state: 'unknown' }
  }
}

export function useServiceStore() {
  if (refCount === 0) {
    poll()
    pollTimer = setInterval(poll, 2000)
  }
  refCount++

  onUnmounted(() => {
    refCount--
    if (refCount === 0 && pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  })

  return {
    serviceStatus,
    refresh: poll,
  }
}
