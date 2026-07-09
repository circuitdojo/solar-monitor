import { useEffect, useMemo, useRef, useState } from 'preact/hooks'
import { Link } from 'wouter'
import { DeviceData } from '../../types/ts'
import { DeviceSelect, useDeviceSelection, useDevices } from './device-select'

// One polled reading, flattened for charting
type Sample = {
  t: number
  load: number | null
  pv: number | null
  soc: number | null
  vbat: number | null
  ibat: number | null
  gridV: number | null
  gridF: number | null
  temp: number | null
  custom: { [key: string]: number }
}

const BUFFER = 720 // ~1h at 5s polls

function toSample(d: DeviceData): Sample {
  const m = d.metrics
  return {
    t: Date.parse(d.timestamp),
    load: m.outputPowerWatts,
    pv: m.pvPowerWatts,
    soc: m.batterySocPercentage,
    vbat: m.batteryVoltage,
    ibat: m.batteryCurrent,
    gridV: m.gridVoltage,
    gridF: m.gridFrequency,
    temp: m.deviceTemperatureCelsius,
    custom: m.customMetrics || {},
  }
}

function useLiveData(deviceId: string | null) {
  const [samples, setSamples] = useState<Sample[]>([])
  const [wsUp, setWsUp] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setSamples([])
    setWsUp(false)
    setError(null)
    if (!deviceId) return

    let cancelled = false
    let ws: WebSocket | null = null
    let retry: ReturnType<typeof setTimeout> | null = null

    async function init() {
      try {
        const hist: DeviceData[] = await fetch(`/api/v1/devices/${deviceId}/data?limit=${BUFFER}`).then(r => r.json())
        if (cancelled) return
        setSamples(hist.map(toSample).sort((a, b) => a.t - b.t))
        connect()
      } catch (e) {
        if (!cancelled) setError(String(e))
      }
    }

    function connect() {
      const proto = location.protocol === 'https:' ? 'wss' : 'ws'
      ws = new WebSocket(`${proto}://${location.host}/api/v1/ws`)
      ws.onopen = () => setWsUp(true)
      ws.onmessage = ev => {
        try {
          const env = JSON.parse(ev.data)
          if (env.messageType !== 'device_data' || env.data?.deviceId !== deviceId) return
          const s = toSample(env.data as DeviceData)
          setSamples(prev => [...prev.filter(p => p.t !== s.t), s].slice(-BUFFER))
        } catch { /* heartbeat or malformed frame */ }
      }
      ws.onclose = () => {
        setWsUp(false)
        if (!cancelled) retry = setTimeout(connect, 3000)
      }
    }

    init()
    return () => {
      cancelled = true
      if (retry) clearTimeout(retry)
      ws?.close()
    }
  }, [deviceId])

  return { samples, wsUp, error }
}

// --- formatting ---

const fmtW = (v: number | null) => (v == null ? '—' : v >= 10000 ? `${(v / 1000).toFixed(1)} kW` : `${Math.round(v)} W`)
const fmtV = (v: number | null) => (v == null ? '—' : `${v.toFixed(1)} V`)
const fmtTime = (t: number) => new Date(t).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })

// --- charts ---

function useSize<T extends Element>() {
  const ref = useRef<T>(null)
  const [w, setW] = useState(0)
  useEffect(() => {
    if (!ref.current) return
    const ro = new ResizeObserver(es => setW(es[0].contentRect.width))
    ro.observe(ref.current)
    return () => ro.disconnect()
  }, [])
  return { ref, w }
}

type SeriesDef = { key: string; label: string; color: string; format: (v: number | null) => string }

// Multi-series line chart with area wash, end dots, crosshair + tooltip.
function LineChart(props: {
  points: { t: number; values: (number | null)[] }[]
  series: SeriesDef[]
  height?: number
  yTicks?: number
  unitHint?: string
}) {
  const { points, series } = props
  const h = props.height ?? 140
  const { ref, w } = useSize<HTMLDivElement>()
  const [hover, setHover] = useState<number | null>(null)

  const pad = { l: 34, r: 10, t: 8, b: 18 }
  const iw = Math.max(0, w - pad.l - pad.r)
  const ih = h - pad.t - pad.b

  const flat = points.flatMap(p => p.values).filter((v): v is number => v != null)
  const dataLo = flat.length ? Math.min(...flat) : 0
  let lo = dataLo
  let hi = flat.length ? Math.max(...flat) : 1
  if (hi - lo < 1e-9) { lo -= 1; hi += 1 }
  const span = hi - lo
  lo -= span * 0.08
  hi += span * 0.08
  if (lo < 0 && dataLo >= 0) lo = 0
  const t0 = points.length ? points[0].t : 0
  const t1 = points.length ? points[points.length - 1].t : 1
  const x = (t: number) => pad.l + (t1 === t0 ? iw : ((t - t0) / (t1 - t0)) * iw)
  const y = (v: number) => pad.t + ih - ((v - lo) / (hi - lo)) * ih

  // Clean tick values: 1/2/5 × 10^n steps within the domain
  const { ticks, tickStep } = useMemo(() => {
    const n = props.yTicks ?? 3
    const raw = (hi - lo) / n
    const mag = Math.pow(10, Math.floor(Math.log10(raw)))
    const step = [1, 2, 5, 10].map(m => m * mag).find(s => s >= raw) || raw
    const out: number[] = []
    for (let v = Math.ceil(lo / step) * step; v <= hi + 1e-9; v += step) out.push(v)
    return { ticks: out, tickStep: step }
  }, [lo, hi, props.yTicks])

  function onMove(e: PointerEvent) {
    if (!points.length || !ref.current) return
    const px = e.clientX - ref.current.getBoundingClientRect().left
    let best = 0
    let bestD = Infinity
    points.forEach((p, i) => {
      const d = Math.abs(x(p.t) - px)
      if (d < bestD) { bestD = d; best = i }
    })
    setHover(best)
  }

  const hp = hover != null ? points[hover] : null
  const tickFmt = (v: number) => {
    if (Math.abs(hi) >= 10000) return `${(v / 1000).toFixed(1)}k`
    const decimals = tickStep >= 1 ? 0 : tickStep >= 0.1 ? 1 : 2
    return v.toLocaleString(undefined, { minimumFractionDigits: decimals, maximumFractionDigits: decimals })
  }

  return (
    <div ref={ref} class="relative" style={{ height: `${h}px` }} onPointerMove={onMove as any} onPointerLeave={() => setHover(null)}>
      {w > 0 && points.length > 1 && (
        <svg width={w} height={h} class="block">
          {ticks.map(tv => (
            <g>
              <line x1={pad.l} x2={w - pad.r} y1={y(tv)} y2={y(tv)} stroke="var(--vz-grid)" stroke-width="1" />
              <text x={pad.l - 6} y={y(tv) + 3} text-anchor="end" font-size="10" fill="var(--vz-ink-3)">{tickFmt(tv)}</text>
            </g>
          ))}
          <text x={pad.l} y={h - 4} font-size="10" fill="var(--vz-ink-3)">{fmtTime(t0)}</text>
          <text x={w - pad.r} y={h - 4} text-anchor="end" font-size="10" fill="var(--vz-ink-3)">{fmtTime(t1)}</text>
          {series.map((s, si) => {
            const segs = points.map(p => (p.values[si] == null ? null : `${x(p.t)},${y(p.values[si]!)}`))
            const path = segs.filter(Boolean).join(' ')
            const last = [...points].reverse().find(p => p.values[si] != null)
            return (
              <g>
                <polygon
                  points={`${pad.l},${pad.t + ih} ${path} ${x(t1)},${pad.t + ih}`}
                  fill={s.color}
                  opacity="0.1"
                />
                <polyline points={path} fill="none" stroke={s.color} stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
                {last && last.values[si] != null && (
                  <circle cx={x(last.t)} cy={y(last.values[si]!)} r="4" fill={s.color} stroke="var(--vz-surface)" stroke-width="2" />
                )}
              </g>
            )
          })}
          {hp && <line x1={x(hp.t)} x2={x(hp.t)} y1={pad.t} y2={pad.t + ih} stroke="var(--vz-baseline)" stroke-width="1" />}
          {hp && series.map((s, si) => hp.values[si] != null && (
            <circle cx={x(hp.t)} cy={y(hp.values[si]!)} r="4" fill={s.color} stroke="var(--vz-surface)" stroke-width="2" />
          ))}
        </svg>
      )}
      {hp && (
        <div
          class="absolute z-10 rounded px-2.5 py-1.5 text-xs shadow-md pointer-events-none"
          style={{
            left: `${Math.min(Math.max(x(hp.t) + 10, 0), Math.max(w - 130, 0))}px`,
            top: '4px',
            background: 'var(--vz-surface)',
            border: '1px solid var(--vz-border)',
            color: 'var(--vz-ink)',
          }}
        >
          <div style={{ color: 'var(--vz-ink-3)' }}>{new Date(hp.t).toLocaleTimeString()}</div>
          {series.map((s, si) => (
            <div class="flex items-center gap-1.5 whitespace-nowrap">
              <span style={{ background: s.color, width: '10px', height: '2px', display: 'inline-block' }} />
              <span class="font-semibold">{s.format(hp.values[si])}</span>
              <span style={{ color: 'var(--vz-ink-2)' }}>{s.label}</span>
            </div>
          ))}
        </div>
      )}
      {points.length <= 1 && (
        <div class="h-full flex items-center justify-center text-sm" style={{ color: 'var(--vz-ink-3)' }}>
          Collecting data…
        </div>
      )}
    </div>
  )
}

function Sparkline({ points, color }: { points: { t: number; v: number | null }[]; color: string }) {
  const { ref, w } = useSize<HTMLDivElement>()
  const h = 36
  const vals = points.map(p => p.v).filter((v): v is number => v != null)
  let lo = vals.length ? Math.min(...vals) : 0
  let hi = vals.length ? Math.max(...vals) : 1
  if (hi - lo < 1e-9) hi += 1 // flat series sits at the bottom, not mid-air
  const t0 = points.length ? points[0].t : 0
  const t1 = points.length ? points[points.length - 1].t : 1
  const x = (t: number) => (t1 === t0 ? w : ((t - t0) / (t1 - t0)) * (w - 6))
  const y = (v: number) => 3 + (h - 6) - ((v - lo) / (hi - lo)) * (h - 6)
  const path = points.filter(p => p.v != null).map(p => `${x(p.t)},${y(p.v!)}`).join(' ')
  const last = [...points].reverse().find(p => p.v != null)
  return (
    <div ref={ref} style={{ height: `${h}px` }}>
      {w > 0 && points.length > 1 && (
        <svg width={w} height={h} class="block">
          <polygon points={`0,${h} ${path} ${x(t1)},${h}`} fill={color} opacity="0.1" />
          <polyline points={path} fill="none" stroke={color} stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
          {last && last.v != null && (
            <circle cx={x(last.t)} cy={y(last.v)} r="4" fill={color} stroke="var(--vz-surface)" stroke-width="2" />
          )}
        </svg>
      )}
    </div>
  )
}

// --- tiles & cards ---

function Card(props: { title?: string; children: any; class?: string }) {
  return (
    <div
      class={'rounded-lg p-4 ' + (props.class || '')}
      style={{ background: 'var(--vz-surface)', border: '1px solid var(--vz-border)' }}
    >
      {props.title && (
        <div class="text-sm font-medium mb-2" style={{ color: 'var(--vz-ink-2)' }}>{props.title}</div>
      )}
      {props.children}
    </div>
  )
}

function StatTile(props: {
  label: string
  value: string
  sub?: string
  color: string
  spark?: { t: number; v: number | null }[]
}) {
  return (
    <Card>
      <div class="flex items-center gap-2 text-sm" style={{ color: 'var(--vz-ink-2)' }}>
        <span class="inline-block rounded-full" style={{ width: '8px', height: '8px', background: props.color }} />
        {props.label}
      </div>
      <div class="text-3xl font-semibold mt-1" style={{ color: 'var(--vz-ink)' }}>{props.value}</div>
      {props.sub && <div class="text-sm mt-0.5" style={{ color: 'var(--vz-ink-3)' }}>{props.sub}</div>}
      {props.spark && <div class="mt-2"><Sparkline points={props.spark} color={props.color} /></div>}
    </Card>
  )
}

function SocMeter({ soc }: { soc: number }) {
  return (
    <div class="rounded-full overflow-hidden mt-2" style={{ height: '8px', background: 'var(--vz-batt-track)' }}>
      <div class="h-full rounded-full" style={{ width: `${Math.min(100, Math.max(0, soc))}%`, background: 'var(--vz-batt)' }} />
    </div>
  )
}

function EnergyBars({ rows }: { rows: { label: string; kwh: number }[] }) {
  const max = Math.max(0.1, ...rows.map(r => r.kwh))
  return (
    <div class="space-y-2">
      {rows.map(r => (
        <div class="flex items-center gap-2 text-sm">
          <div class="w-24 shrink-0" style={{ color: 'var(--vz-ink-2)' }}>{r.label}</div>
          <div class="flex-1 flex items-center gap-2">
            <div
              class="h-4"
              style={{
                width: `${(r.kwh / max) * 100}%`,
                minWidth: r.kwh > 0 ? '3px' : '0',
                background: 'var(--vz-seq)',
                borderRadius: '0 4px 4px 0',
              }}
            />
            <span class="tabular-nums whitespace-nowrap" style={{ color: 'var(--vz-ink)' }}>{r.kwh.toFixed(1)} kWh</span>
          </div>
        </div>
      ))}
    </div>
  )
}

function FlowStrip({ s }: { s: Sample }) {
  const pvW = s.pv ?? 0
  const imp = s.custom['import_power'] ?? 0
  const exp = s.custom['export_power'] ?? 0
  const battW = s.vbat != null && s.ibat != null ? s.vbat * s.ibat : null
  const node = (color: string, name: string, detail: string) => (
    <div class="flex items-center gap-2">
      <span class="inline-block rounded-full" style={{ width: '10px', height: '10px', background: color }} />
      <div>
        <div class="text-sm" style={{ color: 'var(--vz-ink-2)' }}>{name}</div>
        <div class="font-semibold" style={{ color: 'var(--vz-ink)' }}>{detail}</div>
      </div>
    </div>
  )
  return (
    <div class="flex flex-wrap items-center gap-x-8 gap-y-3">
      {node('var(--vz-pv)', 'Solar', fmtW(pvW))}
      <span style={{ color: 'var(--vz-ink-3)' }}>→</span>
      {node('var(--vz-load)', 'Load', fmtW(s.load))}
      <span style={{ color: 'var(--vz-ink-3)' }}>←</span>
      {node('var(--vz-grid-s)', 'Grid', exp > imp ? `exporting ${fmtW(exp)}` : imp > 0 ? `importing ${fmtW(imp)}` : 'idle')}
      <span style={{ color: 'var(--vz-ink-3)' }}>·</span>
      {node(
        'var(--vz-batt)',
        'Battery',
        battW == null ? '—' : Math.abs(battW) < 15 ? 'idle' : battW > 0 ? `charging ${fmtW(battW)}` : `discharging ${fmtW(-battW)}`,
      )}
      {s.custom['ac_input_is_generator'] === 1 && (
        <>
          <span style={{ color: 'var(--vz-ink-3)' }}>·</span>
          {node('var(--vz-gen)', 'Generator', (s.custom['gen_power'] ?? 0) > 0 ? fmtW(s.custom['gen_power']) : 'off')}
        </>
      )}
    </div>
  )
}

// --- page ---

export function DashboardPage() {
  const { devices, error: devicesError } = useDevices()
  const { device, select } = useDeviceSelection(devices)
  const { samples, wsUp, error: dataError } = useLiveData(device?.id ?? null)
  const error = devicesError || dataError
  const latest = samples.length ? samples[samples.length - 1] : null
  const stale = latest ? Date.now() - latest.t > 30_000 : true

  const spark = (pick: (s: Sample) => number | null) => samples.map(s => ({ t: s.t, v: pick(s) }))
  const powerPoints = useMemo(
    () => samples.map(s => ({ t: s.t, values: [s.load, s.pv] })),
    [samples],
  )
  const socPoints = useMemo(
    () => samples.map(s => ({ t: s.t, values: [s.soc] })),
    [samples],
  )

  const c = latest?.custom || {}
  const solarDay = (c['pv1_day_kwh'] ?? 0) + (c['pv2_day_kwh'] ?? 0) + (c['pv3_day_kwh'] ?? 0)
  const energyRows = latest
    ? [
        { label: 'Solar', kwh: solarDay },
        { label: 'Load', kwh: c['load_day_kwh'] ?? 0 },
        { label: 'Charged', kwh: c['charge_day_kwh'] ?? 0 },
        { label: 'Discharged', kwh: c['discharge_day_kwh'] ?? 0 },
        { label: 'Imported', kwh: c['import_day_kwh'] ?? 0 },
        { label: 'Exported', kwh: c['export_day_kwh'] ?? 0 },
        ...(c['ac_input_is_generator'] === 1 ? [{ label: 'Generator', kwh: c['gen_day_kwh'] ?? 0 }] : []),
      ]
    : []

  const powerSeries: SeriesDef[] = [
    { key: 'load', label: 'Load', color: 'var(--vz-load)', format: fmtW },
    { key: 'pv', label: 'Solar', color: 'var(--vz-pv)', format: fmtW },
  ]
  const socSeries: SeriesDef[] = [
    { key: 'soc', label: 'SOC', color: 'var(--vz-batt)', format: v => (v == null ? '—' : `${Math.round(v)}%`) },
  ]

  return (
    <div class="p-4 md:p-6 space-y-4 max-w-6xl mx-auto">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <div class="flex items-center gap-3">
          <h1 class="text-xl font-semibold" style={{ color: 'var(--vz-ink)' }}>{device?.name || 'Solar Monitor'}</h1>
          <DeviceSelect devices={devices || []} selected={device?.id ?? null} onSelect={select} />
          <span
            class="inline-flex items-center gap-1.5 text-xs px-2 py-0.5 rounded-full"
            style={{
              color: !stale && wsUp ? 'var(--vz-good-text)' : 'var(--vz-crit)',
              background: 'var(--vz-surface)',
              border: '1px solid var(--vz-border)',
            }}
          >
            <span class="inline-block rounded-full" style={{ width: '7px', height: '7px', background: !stale && wsUp ? 'var(--vz-good)' : 'var(--vz-crit)' }} />
            {!stale && wsUp ? 'Live' : wsUp ? 'Stale data' : 'Disconnected'}
          </span>
          {latest && <span class="text-xs" style={{ color: 'var(--vz-ink-3)' }}>updated {new Date(latest.t).toLocaleTimeString()}</span>}
        </div>
        <div class="flex items-center gap-4">
          <Link href="/settings"><a class="text-sm hover:underline" style={{ color: 'var(--vz-load)' }}>Settings</a></Link>
          <Link href="/devices"><a class="text-sm hover:underline" style={{ color: 'var(--vz-load)' }}>Devices</a></Link>
        </div>
      </div>

      {error && <Card><div style={{ color: 'var(--vz-crit)' }}>Error: {error}</div></Card>}
      {!error && devices != null && !device && <Card><div style={{ color: 'var(--vz-ink-2)' }}>No devices configured yet — add one on the Devices page.</div></Card>}

      {device && (
        <>
          <div class="grid gap-3 grid-cols-1 sm:grid-cols-2 lg:grid-cols-4">
            <StatTile
              label="Load"
              value={fmtW(latest?.load ?? null)}
              sub={latest ? `today ${(c['load_day_kwh'] ?? 0).toFixed(1)} kWh` : undefined}
              color="var(--vz-load)"
              spark={spark(s => s.load)}
            />
            <StatTile
              label="Solar"
              value={fmtW(latest?.pv ?? null)}
              sub={latest ? `today ${solarDay.toFixed(1)} kWh` : undefined}
              color="var(--vz-pv)"
              spark={spark(s => s.pv)}
            />
            <Card>
              <div class="flex items-center gap-2 text-sm" style={{ color: 'var(--vz-ink-2)' }}>
                <span class="inline-block rounded-full" style={{ width: '8px', height: '8px', background: 'var(--vz-batt)' }} />
                Battery
              </div>
              <div class="text-3xl font-semibold mt-1" style={{ color: 'var(--vz-ink)' }}>
                {latest?.soc != null ? `${Math.round(latest.soc)}%` : '—'}
              </div>
              <div class="text-sm mt-0.5" style={{ color: 'var(--vz-ink-3)' }}>
                {fmtV(latest?.vbat ?? null)}{latest?.ibat != null ? ` · ${latest.ibat.toFixed(1)} A` : ''}
              </div>
              {latest?.soc != null && <SocMeter soc={latest.soc} />}
            </Card>
            <StatTile
              label="Grid"
              value={fmtV(latest?.gridV ?? null)}
              sub={latest?.gridF != null ? `${latest.gridF.toFixed(2)} Hz` : undefined}
              color="var(--vz-grid-s)"
              spark={spark(s => s.gridV)}
            />
          </div>

          <Card title="Power — last hour">
            <div class="flex items-center gap-4 text-xs mb-1" style={{ color: 'var(--vz-ink-2)' }}>
              {powerSeries.map(s => (
                <span class="inline-flex items-center gap-1.5">
                  <span style={{ background: s.color, width: '12px', height: '2px', display: 'inline-block' }} />
                  {s.label}
                </span>
              ))}
            </div>
            <LineChart points={powerPoints} series={powerSeries} height={180} />
          </Card>

          <div class="grid gap-3 grid-cols-1 lg:grid-cols-2">
            <Card title="Battery SOC — last hour">
              <LineChart points={socPoints} series={socSeries} height={140} />
            </Card>
            <Card title="Energy today">
              {energyRows.length ? <EnergyBars rows={energyRows} /> : <div style={{ color: 'var(--vz-ink-3)' }}>Waiting for data…</div>}
            </Card>
          </div>

          <div class="grid gap-3 grid-cols-1 lg:grid-cols-2">
            <Card title="Power flow">
              {latest ? <FlowStrip s={latest} /> : <div style={{ color: 'var(--vz-ink-3)' }}>Waiting for data…</div>}
            </Card>
            {c['ac_input_is_generator'] === 1 && <Card title="Generator">
              {latest ? (
                <div class="flex flex-wrap gap-x-8 gap-y-2 text-sm">
                  {[
                    ['Power', fmtW(c['gen_power'] ?? 0)],
                    ['Voltage', fmtV(c['gen_voltage'] ?? 0)],
                    ['Frequency', c['gen_frequency'] ? `${c['gen_frequency'].toFixed(2)} Hz` : '—'],
                    ['Today', `${(c['gen_day_kwh'] ?? 0).toFixed(1)} kWh`],
                    ['Total', `${(c['gen_total_kwh'] ?? 0).toFixed(1)} kWh`],
                  ].map(([label, v]) => (
                    <div>
                      <div style={{ color: 'var(--vz-ink-2)' }}>{label}</div>
                      <div class="text-xl font-semibold" style={{ color: 'var(--vz-ink)' }}>{v}</div>
                    </div>
                  ))}
                </div>
              ) : (
                <div style={{ color: 'var(--vz-ink-3)' }}>Waiting for data…</div>
              )}
            </Card>}
            <Card title="Temperatures">
              {latest ? (
                <div class="flex flex-wrap gap-x-8 gap-y-2 text-sm">
                  {[
                    ['Inverter', latest.temp],
                    ['Heatsink', c['heatsink_temp1_c'] ?? null],
                    ['Battery', c['battery_temp_c'] ?? null],
                  ].map(([label, v]) => (
                    <div>
                      <div style={{ color: 'var(--vz-ink-2)' }}>{label}</div>
                      <div class="text-xl font-semibold" style={{ color: 'var(--vz-ink)' }}>
                        {v == null ? '—' : `${v} °C`}
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div style={{ color: 'var(--vz-ink-3)' }}>Waiting for data…</div>
              )}
            </Card>
          </div>
        </>
      )}
    </div>
  )
}
