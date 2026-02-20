import { ref, onUnmounted, watch } from 'vue'
import { createClashWS } from '@/api/websocket'
import { useConfigStore } from './config'
import type { TrafficData, MemoryData } from '@/types'
import type ReconnectingWebSocket from 'reconnecting-websocket'

const MAX_POINTS = 60

const trafficHistory = ref<{ up: number; down: number; time: string }[]>([])
const currentTraffic = ref<TrafficData>({ up: 0, down: 0 })
const memory = ref<MemoryData>({ inuse: 0, oslimit: 0 })

let trafficWs: ReconnectingWebSocket | null = null
let memoryWs: ReconnectingWebSocket | null = null
let refCount = 0

export function useOverviewStore() {
  const { activeClashApiId } = useConfigStore()

  function start() {
    if (!trafficWs) {
      trafficWs = createClashWS('/traffic', (data: TrafficData) => {
        currentTraffic.value = data
        trafficHistory.value.push({
          up: data.up,
          down: data.down,
          time: new Date().toLocaleTimeString(),
        })
        if (trafficHistory.value.length > MAX_POINTS) {
          trafficHistory.value.shift()
        }
      })
    }
    if (!memoryWs) {
      memoryWs = createClashWS('/memory', (data: MemoryData) => {
        memory.value = data
      })
    }
  }

  function stop() {
    trafficWs?.close()
    trafficWs = null
    memoryWs?.close()
    memoryWs = null
  }

  refCount++
  const unwatchApi = watch(
    () => activeClashApiId.value,
    () => {
      if (trafficWs || memoryWs) {
        stop()
        start()
      }
    },
  )
  onUnmounted(() => {
    unwatchApi()
    refCount--
    if (refCount === 0) stop()
  })

  return {
    trafficHistory,
    currentTraffic,
    memory,
    start,
    stop,
  }
}
