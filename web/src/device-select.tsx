import { useEffect, useState } from 'preact/hooks'
import { CombinedInfoDto, DeviceListItemDto } from '../../types/ts'

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

// Whether the enabled devices form a combinable parallel system
// (`undefined` while loading).
export function useCombinedInfo() {
  const [info, setInfo] = useState<CombinedInfoDto | null | undefined>(undefined)

  useEffect(() => {
    let cancelled = false
    fetch('/api/v1/combined')
      .then(r => (r.ok ? r.json() : Promise.reject(r.statusText)))
      .then(d => { if (!cancelled) setInfo(d) })
      .catch(() => { if (!cancelled) setInfo(null) })
    return () => { cancelled = true }
  }, [])

  return info
}

/** Selection value for the combined parallel-system view (dashboard only). */
export const COMBINED_ID = 'combined'

export function combinedLabel(info: CombinedInfoDto) {
  return `System (${info.memberIds.length} inverters)`
}

// Selection persisted in localStorage under a per-page key (the dashboard
// and settings pick independently). When `combinedAvailable`, the combined
// view is the default and only an explicit device choice overrides it;
// otherwise the fallback is the first enabled device. Nothing is selected
// until `devices` has loaded.
export function useDeviceSelection(
  storageKey: string,
  devices: DeviceListItemDto[] | null,
  combinedAvailable = false,
) {
  const [storedId, setStoredId] = useState<string | null>(() => localStorage.getItem(storageKey))
  const select = (id: string) => {
    localStorage.setItem(storageKey, id)
    setStoredId(id)
  }
  const chosen = devices?.find(d => d.id === storedId) ?? null
  const combined = !!devices && combinedAvailable && !chosen
  const device = devices && !combined
    ? chosen || devices.find(d => d.enabled) || devices[0] || null
    : null
  return { device, combined, select }
}

export function DeviceSelect({ devices, selected, onSelect, combinedLabel }: {
  devices: DeviceListItemDto[]
  selected: string | null
  onSelect: (id: string) => void
  /** Label for the combined option; omitted ⇒ no combined option. */
  combinedLabel?: string
}) {
  if (devices.length <= 1) return null
  return (
    <select
      class="vz-input"
      style={{ width: 'auto' }}
      aria-label="Device"
      value={selected ?? ''}
      onChange={(e: any) => onSelect(e.target.value)}
    >
      {combinedLabel && <option value={COMBINED_ID}>{combinedLabel}</option>}
      {devices.map(d => <option value={d.id}>{d.name}</option>)}
    </select>
  )
}
