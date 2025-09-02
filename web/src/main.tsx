import { render } from 'preact'
import { useEffect, useState } from 'preact/hooks'
import { Link, Route, Router } from 'wouter'
import './index.css'
import { DeviceListItemDto } from '../../types/ts'

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
  const [showAdd, setShowAdd] = useState(false)
  const [refreshKey, setRefreshKey] = useState(0)
  const reload = () => setRefreshKey(k => k + 1)
  useEffect(() => { /* trigger reload when add closes */ }, [refreshKey])
  const onAdded = () => { setShowAdd(false); setTimeout(reload, 200) }

  async function remove(id: string) {
    if (!confirm(`Remove device ${id}?`)) return
    const res = await fetch(`/api/v1/devices/${id}`, { method: 'DELETE' })
    if (res.ok) reload()
  }
  if (loading) return <div class="p-6">Loading…</div>
  if (error) return <div class="p-6 text-red-600">Error: {error}</div>
  return (
    <div class="p-6 space-y-4">
      <div class="flex items-center justify-between">
        <h1 class="text-xl font-semibold">Devices</h1>
        <div class="space-x-3">
          <button class="px-3 py-1.5 rounded bg-blue-600 text-white" onClick={() => setShowAdd(true)}>Add Device</button>
          <Link href="/"><a class="text-blue-600 hover:underline">Dashboard</a></Link>
        </div>
      </div>
      <div class="grid gap-3">
        {(data || []).map(d => (
          <div class="rounded border bg-white p-4 shadow-sm flex items-center justify-between">
            <div>
              <div class="font-medium">{d.name}</div>
              <div class="text-sm text-slate-500">{d.deviceType} · {d.protocolName}</div>
            </div>
            <div class="text-sm flex items-center gap-3">
              <span class={"px-2 py-1 rounded " + (d.isPolling ? 'bg-green-100 text-green-800' : 'bg-slate-100 text-slate-700')}>
                {d.isPolling ? 'Polling' : 'Stopped'}
              </span>
              <button class="px-2 py-1 rounded bg-red-50 text-red-700 hover:bg-red-100" onClick={() => remove(d.id)}>Remove</button>
            </div>
          </div>
        ))}
      </div>
      {showAdd && <AddDeviceModal onClose={() => setShowAdd(false)} onSaved={onAdded} />}
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

type AddDeviceProps = { onClose: () => void; onSaved: () => void }
function AddDeviceModal({ onClose, onSaved }: AddDeviceProps) {
  const [serialPorts, setSerialPorts] = useState<string[]>([])
  const [id, setId] = useState('')
  const [name, setName] = useState('')
  const [deviceType, setDeviceType] = useState<DeviceListItemDto['deviceType']>('SolarInverter')
  const [protocolName, setProtocolName] = useState('eg4-6000xp-modbus')
  const [serialPort, setSerialPort] = useState('')
  const [baudRate, setBaudRate] = useState('9600')
  const [unitId, setUnitId] = useState('1')
  const [pollInterval, setPollInterval] = useState(30)
  const [enabled, setEnabled] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    fetch('/api/v1/system/serial-ports').then(r => r.json()).then(setSerialPorts).catch(() => setSerialPorts([]))
  }, [])

  async function save() {
    setSaving(true); setError(null)
    const body = {
      id, name,
      deviceType,
      protocolName,
      enabled,
      pollIntervalSeconds: pollInterval,
      connectionParams: { serial_port: serialPort, baud_rate: baudRate, unit_id: unitId }
    }
    const res = await fetch('/api/v1/devices', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body)
    })
    setSaving(false)
    if (res.ok) onSaved(); else setError(await res.text())
  }

  return (
    <div class="fixed inset-0 bg-black/30 flex items-center justify-center">
      <div class="bg-white rounded shadow-lg w-[520px] max-w-[95vw]">
        <div class="px-4 py-3 border-b font-semibold">Add Device</div>
        <div class="p-4 space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block text-sm text-slate-600">ID</label>
              <input class="w-full border rounded px-2 py-1" value={id} onInput={(e: any) => setId(e.target.value)} />
            </div>
            <div>
              <label class="block text-sm text-slate-600">Name</label>
              <input class="w-full border rounded px-2 py-1" value={name} onInput={(e: any) => setName(e.target.value)} />
            </div>
            <div>
              <label class="block text-sm text-slate-600">Device Type</label>
              <select class="w-full border rounded px-2 py-1" value={deviceType} onChange={(e: any) => setDeviceType(e.target.value)}>
                <option value="SolarInverter">SolarInverter</option>
                <option value="BatterySystem">BatterySystem</option>
                <option value="ChargeController">ChargeController</option>
                <option value="EnergyMeter">EnergyMeter</option>
              </select>
            </div>
            <div>
              <label class="block text-sm text-slate-600">Protocol</label>
              <input class="w-full border rounded px-2 py-1" value={protocolName} onInput={(e: any) => setProtocolName(e.target.value)} />
            </div>
            <div>
              <label class="block text-sm text-slate-600">Serial Port</label>
              <select class="w-full border rounded px-2 py-1" value={serialPort} onChange={(e: any) => setSerialPort(e.target.value)}>
                <option value="">Select...</option>
                {serialPorts.map(p => <option value={p}>{p}</option>)}
              </select>
            </div>
            <div>
              <label class="block text-sm text-slate-600">Baud Rate</label>
              <input class="w-full border rounded px-2 py-1" value={baudRate} onInput={(e: any) => setBaudRate(e.target.value)} />
            </div>
            <div>
              <label class="block text-sm text-slate-600">Unit ID</label>
              <input class="w-full border rounded px-2 py-1" value={unitId} onInput={(e: any) => setUnitId(e.target.value)} />
            </div>
            <div>
              <label class="block text-sm text-slate-600">Poll Interval (s)</label>
              <input type="number" class="w-full border rounded px-2 py-1" value={pollInterval} onInput={(e: any) => setPollInterval(parseInt(e.target.value || '0'))} />
            </div>
            <div class="flex items-end">
              <label class="inline-flex items-center gap-2 text-sm"><input type="checkbox" checked={enabled} onChange={(e: any) => setEnabled(e.target.checked)} /> Enabled</label>
            </div>
          </div>
          {error && <div class="text-red-600 text-sm">{error}</div>}
        </div>
        <div class="px-4 py-3 border-t flex justify-end gap-2">
          <button class="px-3 py-1.5" onClick={onClose}>Cancel</button>
          <button class={"px-3 py-1.5 rounded text-white " + (saving ? 'bg-slate-400' : 'bg-blue-600')} disabled={saving} onClick={save}>Save</button>
        </div>
      </div>
    </div>
  )
}
