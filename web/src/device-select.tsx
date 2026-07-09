import { useEffect, useState } from 'preact/hooks'
import { DeviceListItemDto } from '../../types/ts'

export function useDevices() {
  const [devices, setDevices] = useState<DeviceListItemDto[] | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    fetch('/api/v1/devices')
      .then(r => (r.ok ? r.json() : Promise.reject(r.statusText)))
      .then(d => { if (!cancelled) setDevices(d) })
      .catch(e => { if (!cancelled) setError(String(e)) })
    return () => { cancelled = true }
  }, [])

  return { devices, error }
}

const STORAGE_KEY = 'selectedDeviceId'

// Selection persisted in localStorage; falls back to the first enabled device.
export function useDeviceSelection(devices: DeviceListItemDto[] | null) {
  const [storedId, setStoredId] = useState<string | null>(() => localStorage.getItem(STORAGE_KEY))
  const select = (id: string) => {
    localStorage.setItem(STORAGE_KEY, id)
    setStoredId(id)
  }
  const device = devices
    ? devices.find(d => d.id === storedId) || devices.find(d => d.enabled) || devices[0] || null
    : null
  return { device, select }
}

export function DeviceSelect({ devices, selected, onSelect }: {
  devices: DeviceListItemDto[]
  selected: string | null
  onSelect: (id: string) => void
}) {
  if (devices.length <= 1) return null
  return (
    <select
      class="vz-input"
      style={{ width: 'auto' }}
      value={selected ?? ''}
      onChange={(e: any) => onSelect(e.target.value)}
    >
      {devices.map(d => <option value={d.id}>{d.name}</option>)}
    </select>
  )
}
