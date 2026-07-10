import { render } from 'preact'
import { useEffect, useState } from 'preact/hooks'
import { Route, Router } from 'wouter'
import './index.css'
import { DeviceListItemDto, ProtocolInfoDto } from '../../types/ts'
import { DashboardPage } from './dashboard'
import { PageShell, PageTitle } from './layout'
import { SettingsPage } from './settings'

function DevicesPage() {
  const [devices, setDevices] = useState<DeviceListItemDto[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [showAdd, setShowAdd] = useState(false)
  const [editing, setEditing] = useState<DeviceListItemDto | null>(null)
  const [refreshKey, setRefreshKey] = useState(0)
  const reload = () => setRefreshKey(k => k + 1)
  const onSaved = () => { setShowAdd(false); setEditing(null); reload() }

  useEffect(() => {
    let cancelled = false
    fetch('/api/v1/devices')
      .then(r => (r.ok ? r.json() : Promise.reject(r.statusText)))
      .then(d => { if (!cancelled) setDevices(d) })
      .catch(e => { if (!cancelled) setError(String(e)) })
    return () => { cancelled = true }
  }, [refreshKey])

  async function remove(id: string) {
    if (!confirm(`Remove device ${id}?`)) return
    const res = await fetch(`/api/v1/devices/${id}`, { method: 'DELETE' })
    if (res.ok) reload()
  }

  return (
    <PageShell
      header={
        <>
          <PageTitle>Devices</PageTitle>
          <button class="vz-btn vz-btn-primary" onClick={() => setShowAdd(true)}>Add Device</button>
        </>
      }
    >
      {error && <div class="vz-card p-4" style={{ color: 'var(--vz-crit)' }}>Error: {error}</div>}
      {!error && devices == null && <div class="p-4" style={{ color: 'var(--vz-ink-3)' }}>Loading…</div>}
      {devices && devices.length === 0 && (
        <div class="vz-card p-4" style={{ color: 'var(--vz-ink-2)' }}>No devices yet — add one to start polling.</div>
      )}
      <div class="grid gap-3">
        {(devices || []).map(d => (
          <div class="vz-card p-4 flex items-center justify-between flex-wrap gap-3">
            <div>
              <div class="font-medium" style={{ color: 'var(--vz-ink)' }}>{d.name}</div>
              <div class="text-sm" style={{ color: 'var(--vz-ink-3)' }}>
                {d.protocolName} · {d.connectionParams['serial_port'] || '—'}
                {d.connectionParams['baud_rate'] ? ` @ ${d.connectionParams['baud_rate']} baud` : ''}
                {d.connectionParams['unit_id'] ? ` · unit ${d.connectionParams['unit_id']}` : ''}
                {` · every ${d.pollIntervalSeconds}s`}
              </div>
            </div>
            <div class="text-sm flex items-center gap-3">
              <span
                class="inline-flex items-center gap-1.5 text-xs px-2 py-0.5 rounded-full"
                style={{
                  color: d.isPolling ? 'var(--vz-good-text)' : 'var(--vz-ink-2)',
                  border: '1px solid var(--vz-border)',
                }}
              >
                <span
                  class="inline-block rounded-full"
                  style={{ width: '7px', height: '7px', background: d.isPolling ? 'var(--vz-good)' : 'var(--vz-ink-3)' }}
                />
                {d.isPolling ? 'Polling' : 'Stopped'}
              </span>
              <button class="vz-btn vz-btn-ghost" onClick={() => setEditing(d)}>Edit</button>
              <button class="vz-btn vz-btn-danger" onClick={() => remove(d.id)}>Remove</button>
            </div>
          </div>
        ))}
      </div>
      {showAdd && <DeviceModal onClose={() => setShowAdd(false)} onSaved={onSaved} />}
      {editing && <DeviceModal device={editing} onClose={() => setEditing(null)} onSaved={onSaved} />}
    </PageShell>
  )
}

function App() {
  return (
    <Router>
      <Route path="/" component={DashboardPage} />
      <Route path="/devices" component={DevicesPage} />
      <Route path="/settings" component={SettingsPage} />
    </Router>
  )
}

render(<App />, document.getElementById('app')!)

function Field({ label, children }: { label: string; children: any }) {
  return (
    <div>
      <label class="block text-sm mb-1" style={{ color: 'var(--vz-ink-2)' }}>{label}</label>
      {children}
    </div>
  )
}

type DeviceModalProps = { device?: DeviceListItemDto; onClose: () => void; onSaved: () => void }
function DeviceModal({ device, onClose, onSaved }: DeviceModalProps) {
  const editing = device != null
  const [serialPorts, setSerialPorts] = useState<string[]>([])
  const [protocols, setProtocols] = useState<ProtocolInfoDto[]>([])
  const [id, setId] = useState(device?.id ?? '')
  const [name, setName] = useState(device?.name ?? '')
  const [deviceType, setDeviceType] = useState<DeviceListItemDto['deviceType']>(device?.deviceType ?? 'solarInverter')
  const [protocolName, setProtocolName] = useState(device?.protocolName ?? '')
  const [serialPort, setSerialPort] = useState(device?.connectionParams['serial_port'] ?? '')
  const [baudRate, setBaudRate] = useState(device?.connectionParams['baud_rate'] ?? '19200')
  const [unitId, setUnitId] = useState(device?.connectionParams['unit_id'] ?? '1')
  const [pollInterval, setPollInterval] = useState(device?.pollIntervalSeconds ?? 30)
  const [enabled, setEnabled] = useState(device?.enabled ?? false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    fetch('/api/v1/system/serial-ports').then(r => r.json()).then(setSerialPorts).catch(() => setSerialPorts([]))
    fetch('/api/v1/protocols')
      .then(r => r.json())
      .then((ps: ProtocolInfoDto[]) => {
        setProtocols(ps)
        setProtocolName(prev => prev || ps[0]?.protocolName || '')
      })
      .catch(() => setProtocols([]))
  }, [])

  async function save() {
    setSaving(true); setError(null)
    const body = {
      id, name,
      deviceType,
      protocolName,
      enabled,
      pollIntervalSeconds: pollInterval,
      // Keep params this form doesn't edit (e.g. parity from discovery)
      connectionParams: {
        ...(device?.connectionParams ?? {}),
        serial_port: serialPort, baud_rate: baudRate, unit_id: unitId,
      },
    }
    const res = await fetch(editing ? `/api/v1/devices/${device.id}` : '/api/v1/devices', {
      method: editing ? 'PUT' : 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body)
    })
    setSaving(false)
    if (res.ok) onSaved(); else setError(await res.text())
  }

  return (
    <div class="fixed inset-0 flex items-center justify-center p-4" style={{ background: 'rgba(0,0,0,0.4)' }}>
      <div class="vz-card w-[520px] max-w-full">
        <div class="px-4 py-3 font-semibold" style={{ borderBottom: '1px solid var(--vz-border)', color: 'var(--vz-ink)' }}>
          {editing ? `Edit Device — ${device.name}` : 'Add Device'}
        </div>
        <div class="p-4 space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <Field label="ID">
              <input class="vz-input" value={id} disabled={editing} onInput={(e: any) => setId(e.target.value)} />
            </Field>
            <Field label="Name">
              <input class="vz-input" value={name} onInput={(e: any) => setName(e.target.value)} />
            </Field>
            <Field label="Device Type">
              <select class="vz-input" value={deviceType} onChange={(e: any) => setDeviceType(e.target.value)}>
                <option value="solarInverter">Solar Inverter</option>
                <option value="batterySystem">Battery System</option>
                <option value="chargeController">Charge Controller</option>
                <option value="energyMeter">Energy Meter</option>
              </select>
            </Field>
            <Field label="Protocol">
              <select class="vz-input" value={protocolName} onChange={(e: any) => setProtocolName(e.target.value)}>
                {protocols.map(p => <option value={p.protocolName}>{p.name}</option>)}
              </select>
            </Field>
            <Field label="Serial Port">
              <select class="vz-input" value={serialPort} onChange={(e: any) => setSerialPort(e.target.value)}>
                <option value="">Select...</option>
                {serialPorts.map(p => <option value={p}>{p}</option>)}
              </select>
            </Field>
            <Field label="Baud Rate">
              <input class="vz-input" value={baudRate} onInput={(e: any) => setBaudRate(e.target.value)} />
            </Field>
            <Field label="Unit ID">
              <input class="vz-input" value={unitId} onInput={(e: any) => setUnitId(e.target.value)} />
            </Field>
            <Field label="Poll Interval (s)">
              <input type="number" class="vz-input" value={pollInterval} onInput={(e: any) => setPollInterval(parseInt(e.target.value || '0'))} />
            </Field>
            <div class="flex items-end pb-1">
              <label class="inline-flex items-center gap-2 text-sm" style={{ color: 'var(--vz-ink-2)' }}>
                <input type="checkbox" checked={enabled} onChange={(e: any) => setEnabled(e.target.checked)} /> Enabled
              </label>
            </div>
          </div>
          {error && <div class="text-sm" style={{ color: 'var(--vz-crit)' }}>{error}</div>}
        </div>
        <div class="px-4 py-3 flex justify-end gap-2" style={{ borderTop: '1px solid var(--vz-border)' }}>
          <button class="vz-btn vz-btn-ghost" onClick={onClose}>Cancel</button>
          <button class="vz-btn vz-btn-primary" disabled={saving} onClick={save}>Save</button>
        </div>
      </div>
    </div>
  )
}
