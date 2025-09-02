import { render } from 'preact'
import { useEffect, useState } from 'preact/hooks'
import { Link, Route, Router } from 'wouter'
import './index.css'

type DeviceListItemDto = {
  id: string
  name: string
  deviceType: 'SolarInverter' | 'BatterySystem' | 'ChargeController' | 'EnergyMeter'
  protocolName: string
  enabled: boolean
  pollIntervalSeconds: number
  connectionParams: Record<string, string>
  isPolling: boolean
}

function useFetch<T>(url: string) {
  const [data, setData] = useState<T | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let cancelled = false
    setLoading(true)
    fetch(url)
      .then(r => r.ok ? r.json() : Promise.reject(r.statusText))
      .then(d => { if (!cancelled) { setData(d); setLoading(false)} })
      .catch(e => { if (!cancelled) { setError(String(e)); setLoading(false)} })
    return () => { cancelled = true }
  }, [url])
  return { data, loading, error }
}

function DevicesPage() {
  const { data, loading, error } = useFetch<DeviceListItemDto[]>('/api/v1/devices')
  if (loading) return <div class="p-6">Loading…</div>
  if (error) return <div class="p-6 text-red-600">Error: {error}</div>
  return (
    <div class="p-6 space-y-4">
      <div class="flex items-center justify-between">
        <h1 class="text-xl font-semibold">Devices</h1>
        <Link href="/"><a class="text-blue-600 hover:underline">Dashboard</a></Link>
      </div>
      <div class="grid gap-3">
        {(data || []).map(d => (
          <div class="rounded border bg-white p-4 shadow-sm flex items-center justify-between">
            <div>
              <div class="font-medium">{d.name}</div>
              <div class="text-sm text-slate-500">{d.deviceType} · {d.protocolName}</div>
            </div>
            <div class="text-sm">
              <span class={"px-2 py-1 rounded " + (d.isPolling ? 'bg-green-100 text-green-800' : 'bg-slate-100 text-slate-700')}>
                {d.isPolling ? 'Polling' : 'Stopped'}
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

function DashboardPage() {
  const [messages, setMessages] = useState<string[]>([])
  useEffect(() => {
    const proto = location.protocol === 'https:' ? 'wss' : 'ws'
    const ws = new WebSocket(`${proto}://${location.host}/api/v1/ws`)
    ws.onmessage = (ev) => setMessages(m => [ev.data, ...m].slice(0, 20))
    return () => ws.close()
  }, [])
  return (
    <div class="p-6 space-y-4">
      <div class="flex items-center justify-between">
        <h1 class="text-xl font-semibold">Dashboard</h1>
        <Link href="/devices"><a class="text-blue-600 hover:underline">Devices</a></Link>
      </div>
      <div class="grid gap-2">
        {messages.map((m, i) => (
          <pre class="text-xs bg-slate-100 p-2 rounded overflow-auto" key={i}>{m}</pre>
        ))}
      </div>
    </div>
  )
}

function App() {
  return (
    <Router>
      <Route path="/" component={DashboardPage} />
      <Route path="/devices" component={DevicesPage} />
    </Router>
  )
}

render(<App />, document.getElementById('app')!)
