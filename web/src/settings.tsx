import { useEffect, useMemo, useState } from 'preact/hooks'
import { DeviceSettingDto, SettingValueDto } from '../../types/ts'
import { DeviceSelect, useDeviceSelection, useDevices } from './device-select'
import { PageShell, PageTitle } from './layout'

// Editable state per setting: the string form the input holds
function initial(s: SettingValueDto): string {
  switch (s.kind) {
    case 'number': return s.value.toFixed(s.step >= 1 ? 0 : s.step >= 0.1 ? 1 : 2)
    case 'toggle': return String(s.enabled)
    case 'choice': return String(s.value)
    case 'timeWindow': return `${s.start}-${s.end}`
  }
}

type RowState = { draft: string; saving: boolean; error: string | null; saved: boolean }

function SettingRow({ deviceId, s, onStored }: {
  deviceId: string
  s: DeviceSettingDto
  onStored: (s: DeviceSettingDto) => void
}) {
  const [st, setSt] = useState<RowState>({ draft: initial(s.setting), saving: false, error: null, saved: false })
  useEffect(() => setSt({ draft: initial(s.setting), saving: false, error: null, saved: false }), [s])
  const dirty = st.draft !== initial(s.setting)

  async function save(value: string) {
    if (s.confirm && !confirm(`${s.confirm}\n\nWrite ${value} to "${s.label}" now?`)) {
      setSt(p => ({ ...p, draft: initial(s.setting) }))
      return
    }
    setSt(p => ({ ...p, saving: true, error: null, saved: false }))
    const res = await fetch(`/api/v1/devices/${deviceId}/settings/${s.key}`, {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ value }),
    })
    if (res.ok) {
      const stored: DeviceSettingDto = await res.json()
      onStored(stored)
      setSt({ draft: initial(stored.setting), saving: false, error: null, saved: true })
      setTimeout(() => setSt(p => ({ ...p, saved: false })), 2500)
    } else {
      let msg = res.statusText
      try { msg = (await res.json()).details || msg } catch { /* not json */ }
      setSt(p => ({ ...p, saving: false, error: msg }))
    }
  }

  const v = s.setting
  return (
    <div class="py-3 flex items-center justify-between gap-4 flex-wrap" style={{ borderTop: '1px solid var(--vz-border)' }}>
      <div class="min-w-0">
        <div class="text-sm" style={{ color: 'var(--vz-ink)' }}>{s.label}</div>
        {st.error && <div class="text-xs mt-0.5" style={{ color: 'var(--vz-crit)' }}>{st.error}</div>}
        {st.saved && <div class="text-xs mt-0.5" style={{ color: 'var(--vz-good-text)' }}>Saved</div>}
      </div>
      <div class="flex items-center gap-2">
        {v.kind === 'number' && (
          <>
            <input
              type="number"
              class="vz-input"
              style={{ width: '7rem' }}
              min={v.min} max={v.max} step={v.step}
              value={st.draft}
              onInput={(e: any) => setSt(p => ({ ...p, draft: e.target.value }))}
            />
            {v.unit && <span class="text-sm" style={{ color: 'var(--vz-ink-3)' }}>{v.unit}</span>}
          </>
        )}
        {v.kind === 'choice' && (
          <select
            class="vz-input"
            style={{ width: '8rem' }}
            value={st.draft}
            onChange={(e: any) => setSt(p => ({ ...p, draft: e.target.value }))}
          >
            {v.options.map((o, i) => (
              <option value={String(o)}>{v.labels?.[i] ?? `${o}${v.unit ? ` ${v.unit}` : ''}`}</option>
            ))}
          </select>
        )}
        {v.kind === 'timeWindow' && (
          <TimeWindowInput draft={st.draft} onChange={d => setSt(p => ({ ...p, draft: d }))} />
        )}
        {v.kind === 'toggle' ? (
          <button
            role="switch"
            aria-checked={v.enabled}
            disabled={st.saving}
            class="rounded-full transition-colors"
            style={{
              width: '40px', height: '22px', padding: '2px',
              background: v.enabled ? 'var(--vz-good)' : 'var(--vz-baseline)',
            }}
            onClick={() => save(String(!v.enabled))}
          >
            <span
              class="block rounded-full transition-transform"
              style={{
                width: '18px', height: '18px', background: '#fff',
                transform: v.enabled ? 'translateX(18px)' : 'translateX(0)',
              }}
            />
          </button>
        ) : (
          <button
            class="vz-btn vz-btn-primary"
            style={{ visibility: dirty ? 'visible' : 'hidden' }}
            disabled={st.saving || !dirty}
            onClick={() => save(st.draft)}
          >
            {st.saving ? 'Saving…' : 'Save'}
          </button>
        )}
      </div>
    </div>
  )
}

function TimeWindowInput({ draft, onChange }: { draft: string; onChange: (d: string) => void }) {
  const [start = '', end = ''] = draft.split('-')
  return (
    <div class="flex items-center gap-1.5">
      <input type="time" class="vz-input" style={{ width: '7rem' }} value={start}
        onInput={(e: any) => onChange(`${e.target.value}-${end}`)} />
      <span style={{ color: 'var(--vz-ink-3)' }}>–</span>
      <input type="time" class="vz-input" style={{ width: '7rem' }} value={end}
        onInput={(e: any) => onChange(`${start}-${e.target.value}`)} />
    </div>
  )
}

export function SettingsPage() {
  const { devices, error: devicesError } = useDevices()
  const settable = useMemo(() => (devices || []).filter(d => d.supportsSettings), [devices])
  const { device, select } = useDeviceSelection(devices ? settable : null)
  const [settings, setSettings] = useState<DeviceSettingDto[] | null>(null)
  const [settingsError, setSettingsError] = useState<string | null>(null)
  const error = devicesError || settingsError
  const loaded = devices != null
  const deviceId = device?.id ?? null

  useEffect(() => {
    setSettings(null)
    setSettingsError(null)
    if (!deviceId) return
    let cancelled = false
    async function load() {
      try {
        const res = await fetch(`/api/v1/devices/${deviceId}/settings`)
        if (!res.ok) throw new Error((await res.json()).details || res.statusText)
        if (!cancelled) setSettings(await res.json())
      } catch (e) {
        if (!cancelled) setSettingsError(String(e))
      }
    }
    load()
    return () => { cancelled = true }
  }, [deviceId])

  const groups = useMemo(() => {
    const out = new Map<string, DeviceSettingDto[]>()
    for (const s of settings || []) {
      if (!out.has(s.group)) out.set(s.group, [])
      out.get(s.group)!.push(s)
    }
    return [...out.entries()]
  }, [settings])

  function onStored(stored: DeviceSettingDto) {
    setSettings(prev => (prev || []).map(s => (s.key === stored.key ? stored : s)))
  }

  return (
    <PageShell
      header={
        <>
          <PageTitle>Inverter Settings{device ? ` — ${device.name}` : ''}</PageTitle>
          <DeviceSelect devices={settable} selected={deviceId} onSelect={select} />
        </>
      }
    >
      <div class="text-sm" style={{ color: 'var(--vz-ink-3)' }}>
        Values are read from the inverter's holding registers. Writes are range-checked and
        read back to confirm — changes take effect on the inverter immediately.
      </div>
      {error && <div class="vz-card p-4" style={{ color: 'var(--vz-crit)' }}>Error: {error}</div>}
      {!error && !loaded && <div class="p-4" style={{ color: 'var(--vz-ink-3)' }}>Loading…</div>}
      {!error && loaded && !device && (
        <div class="vz-card p-4" style={{ color: 'var(--vz-ink-2)' }}>No devices with configurable settings.</div>
      )}
      {!error && device && settings === null && (
        <div class="p-4" style={{ color: 'var(--vz-ink-3)' }}>Reading settings from inverter…</div>
      )}
      <div class="grid gap-3 grid-cols-1 lg:grid-cols-2 items-start">
        {groups.map(([group, items]) => (
          <div class="vz-card p-4">
            <div class="text-sm font-medium mb-1" style={{ color: 'var(--vz-ink-2)' }}>{group}</div>
            {items.map(s => <SettingRow deviceId={device!.id} s={s} onStored={onStored} />)}
          </div>
        ))}
      </div>
    </PageShell>
  )
}
