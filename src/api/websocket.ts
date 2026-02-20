import ReconnectingWebSocket from 'reconnecting-websocket'
import { useConfigStore } from '@/stores/config'

export function createClashWS(
  path: string,
  onMessage: (data: any) => void,
  params?: Record<string, string>,
): ReconnectingWebSocket {
  const { clashApiUrl, clashApiSecret } = useConfigStore()

  const base = clashApiUrl.value.replace(/^http/, 'ws')
  const url = new URL(path, base)

  if (clashApiSecret.value) {
    url.searchParams.set('token', clashApiSecret.value)
  }
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      url.searchParams.set(k, v)
    }
  }

  const ws = new ReconnectingWebSocket(url.toString(), [], {
    maxReconnectionDelay: 10000,
    minReconnectionDelay: 1000,
    reconnectionDelayGrowFactor: 1.5,
    maxRetries: Infinity,
  })

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data)
      onMessage(data)
    } catch {}
  }

  return ws
}
