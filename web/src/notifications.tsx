import { useEffect, useState } from 'preact/hooks'
import {
  NotificationChannelDto, NotificationChannelKind, NotificationEvent, NotificationLogEntryDto, NotificationRuleDto,
} from '../../types/ts'
import { useDevices } from './device-select'
import { PageShell, PageTitle } from './layout'

// On public ntfy.sh the topic is the only secret — anyone who knows it can
// read and publish. Suggest a random one so users don't pick "solar-alerts".
const randomTopic = () => 'solar-' + crypto.getRandomValues(new Uint8Array(9)).reduce((s, b) => s + 'abcdefghjkmnpqrstuvwxyz23456789'[b % 31], '')

// Kind-specific config fields: [key, label, placeholder, required]
const CHANNEL_FIELDS: Record<NotificationChannelKind, [string, string, string, boolean][]> = {
  ntfy: [
    ['serverUrl', 'Server URL', 'https://ntfy.sh', false],
    ['topic', 'Topic (treat as a secret on ntfy.sh)', '', true],
    ['token', 'Access token (optional)', '', false],
  ],
  email: [
    ['smtpHost', 'SMTP host', 'smtp.fastmail.com', true],
    ['smtpPort', 'SMTP port', '587', false],
    ['username', 'Username', '', true],
    ['password', 'Password', '', true],
    ['from', 'From address', 'solar@example.com', true],
    ['to', 'To address', 'you@example.com', true],
  ],
  pushover: [
    ['userKey', 'User key', '', true],
    ['appToken', 'App token', '', true],
  ],
  webhook: [
    ['url', 'URL', 'https://example.com/hook', true],
  ],
}

const KIND_LABELS: Record<NotificationChannelKind, string> = {
  ntfy: 'ntfy', email: 'Email (SMTP)', pushover: 'Pushover', webhook: 'Webhook',
}

// Event params: [key, label, default]
const EVENT_PARAMS: Record<NotificationEvent, [string, string, number][]> = {
  gridState: [
    ['lostBelow', 'Grid lost below (V)', 80],
    ['restoredAbove', 'Restored above (V)', 100],
  ],
  batteryLow: [
    ['lowBelow', 'Low below (%)', 20],
    ['recoveredAbove', 'Recovered above (%)', 30],
  ],
  deviceOffline: [
    ['offlineAfterSeconds', 'Offline after (s)', 120],
  ],
  generator: [
    ['startAbove', 'Running above (W)', 100],
    ['stopBelow', 'Stopped below (W)', 50],
  ],
}

const EVENT_LABELS: Record<NotificationEvent, string> = {
  gridState: 'Grid lost / restored',
  batteryLow: 'Battery low / recovered',
  deviceOffline: 'Device offline / online',
  generator: 'Generator started / stopped',
}

const slug = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || `id-${Date.now()}`

function Field({ label, children }: { label: string; children: any }) {
  return (
    <div>
      <label class="block text-sm mb-1" style={{ color: 'var(--vz-ink-2)' }}>{label}</label>
      {children}
    </div>
  )
}

function EnabledBadge({ on }: { on: boolean }) {
  return (
    <span
      class="inline-flex items-center gap-1.5 text-xs px-2 py-0.5 rounded-full"
      style={{ color: on ? 'var(--vz-good-text)' : 'var(--vz-ink-2)', border: '1px solid var(--vz-border)' }}
    >
      <span class="inline-block rounded-full" style={{ width: '7px', height: '7px', background: on ? 'var(--vz-good)' : 'var(--vz-ink-3)' }} />
      {on ? 'Enabled' : 'Disabled'}
    </span>
  )
}

export function NotificationsPage() {
  const [channels, setChannels] = useState<NotificationChannelDto[] | null>(null)
  const [rules, setRules] = useState<NotificationRuleDto[] | null>(null)
  const [log, setLog] = useState<NotificationLogEntryDto[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [editChannel, setEditChannel] = useState<NotificationChannelDto | 'new' | null>(null)
  const [editRule, setEditRule] = useState<NotificationRuleDto | 'new' | null>(null)
  const [refreshKey, setRefreshKey] = useState(0)
  const reload = () => setRefreshKey(k => k + 1)

  useEffect(() => {
    let cancelled = false
    Promise.all([
      fetch('/api/v1/notifications/channels').then(r => (r.ok ? r.json() : Promise.reject(r.statusText))),
      fetch('/api/v1/notifications/rules').then(r => (r.ok ? r.json() : Promise.reject(r.statusText))),
      fetch('/api/v1/notifications/log?limit=50').then(r => (r.ok ? r.json() : Promise.reject(r.statusText))),
    ])
      .then(([cs, rs, ls]) => { if (!cancelled) { setChannels(cs); setRules(rs); setLog(ls) } })
      .catch(e => { if (!cancelled) setError(String(e)) })
    return () => { cancelled = true }
  }, [refreshKey])

  async function removeChannel(id: string) {
    if (!confirm(`Remove channel ${id}?`)) return
    const res = await fetch(`/api/v1/notifications/channels/${id}`, { method: 'DELETE' })
    if (res.ok) reload()
  }
  async function removeRule(id: string) {
    if (!confirm(`Remove rule ${id}?`)) return
    const res = await fetch(`/api/v1/notifications/rules/${id}`, { method: 'DELETE' })
    if (res.ok) reload()
  }

  const channelName = (id: string) => channels?.find(c => c.id === id)?.name ?? id

  return (
    <PageShell header={<PageTitle>Notifications</PageTitle>}>
      {error && <div class="vz-card p-4" style={{ color: 'var(--vz-crit)' }}>Error: {error}</div>}

      <div class="flex items-center justify-between">
        <span class="text-sm font-medium" style={{ color: 'var(--vz-ink-2)' }}>Channels</span>
        <button class="vz-btn vz-btn-primary" onClick={() => setEditChannel('new')}>Add Channel</button>
      </div>
      {channels && channels.length === 0 && (
        <div class="vz-card p-4" style={{ color: 'var(--vz-ink-2)' }}>No channels yet — add where alerts should go (ntfy, email, Pushover, webhook).</div>
      )}
      <div class="grid gap-3">
        {(channels || []).map(c => (
          <div class="vz-card p-4 flex items-center justify-between flex-wrap gap-3">
            <div>
              <div class="font-medium" style={{ color: 'var(--vz-ink)' }}>{c.name}</div>
              <div class="text-sm" style={{ color: 'var(--vz-ink-3)' }}>
                {KIND_LABELS[c.kind]}
                {c.kind === 'ntfy' && c.config['topic'] ? ` · ${c.config['topic']}` : ''}
                {c.kind === 'email' && c.config['to'] ? ` · ${c.config['to']}` : ''}
                {c.kind === 'webhook' && c.config['url'] ? ` · ${c.config['url']}` : ''}
              </div>
            </div>
            <div class="text-sm flex items-center gap-3">
              <EnabledBadge on={c.enabled} />
              <TestButton channel={c} />
              <button class="vz-btn vz-btn-ghost" onClick={() => setEditChannel(c)}>Edit</button>
              <button class="vz-btn vz-btn-danger" onClick={() => removeChannel(c.id)}>Remove</button>
            </div>
          </div>
        ))}
      </div>

      <div class="flex items-center justify-between pt-2">
        <span class="text-sm font-medium" style={{ color: 'var(--vz-ink-2)' }}>Rules</span>
        <button class="vz-btn vz-btn-primary" disabled={!channels || channels.length === 0} onClick={() => setEditRule('new')}>Add Rule</button>
      </div>
      {rules && rules.length === 0 && (
        <div class="vz-card p-4" style={{ color: 'var(--vz-ink-2)' }}>No rules yet — pick an event and the channels to alert.</div>
      )}
      <div class="grid gap-3">
        {(rules || []).map(r => (
          <div class="vz-card p-4 flex items-center justify-between flex-wrap gap-3">
            <div>
              <div class="font-medium" style={{ color: 'var(--vz-ink)' }}>{r.name}</div>
              <div class="text-sm" style={{ color: 'var(--vz-ink-3)' }}>
                {EVENT_LABELS[r.event]} · {r.deviceId ?? 'any device'} · → {r.channelIds.map(channelName).join(', ') || 'no channels'}
              </div>
            </div>
            <div class="text-sm flex items-center gap-3">
              <EnabledBadge on={r.enabled} />
              <button class="vz-btn vz-btn-ghost" onClick={() => setEditRule(r)}>Edit</button>
              <button class="vz-btn vz-btn-danger" onClick={() => removeRule(r.id)}>Remove</button>
            </div>
          </div>
        ))}
      </div>

      <div class="flex items-center justify-between pt-2">
        <span class="text-sm font-medium" style={{ color: 'var(--vz-ink-2)' }}>History</span>
        <button class="vz-btn vz-btn-ghost" onClick={reload}>Refresh</button>
      </div>
      {log && log.length === 0 && (
        <div class="vz-card p-4" style={{ color: 'var(--vz-ink-2)' }}>Nothing sent yet.</div>
      )}
      {log && log.length > 0 && (
        <div class="vz-card p-2">
          {log.map(e => (
            <div class="px-2 py-2 flex items-start justify-between gap-4 flex-wrap" style={{ borderTop: '1px solid var(--vz-border)' }}>
              <div class="min-w-0">
                <div class="text-sm flex items-center gap-2" style={{ color: 'var(--vz-ink)' }}>
                  <span
                    class="inline-block rounded-full shrink-0"
                    style={{ width: '7px', height: '7px', background: e.ok ? 'var(--vz-good)' : 'var(--vz-crit)' }}
                  />
                  {e.title}
                </div>
                <div class="text-xs mt-0.5" style={{ color: 'var(--vz-ink-3)' }}>
                  {e.body}
                  {!e.ok && e.error && <span style={{ color: 'var(--vz-crit)' }}> — {e.error}</span>}
                </div>
              </div>
              <div class="text-xs text-right shrink-0" style={{ color: 'var(--vz-ink-3)' }}>
                <div>{new Date(e.timestamp).toLocaleString()}</div>
                <div>via {e.channelName}</div>
              </div>
            </div>
          ))}
        </div>
      )}

      {editChannel && (
        <ChannelModal
          channel={editChannel === 'new' ? null : editChannel}
          onClose={() => setEditChannel(null)}
          onSaved={() => { setEditChannel(null); reload() }}
        />
      )}
      {editRule && channels && (
        <RuleModal
          rule={editRule === 'new' ? null : editRule}
          channels={channels}
          onClose={() => setEditRule(null)}
          onSaved={() => { setEditRule(null); reload() }}
        />
      )}
    </PageShell>
  )
}

function TestButton({ channel }: { channel: NotificationChannelDto }) {
  const [state, setState] = useState<'idle' | 'busy' | 'ok' | 'fail'>('idle')
  const [msg, setMsg] = useState('')
  async function test() {
    setState('busy')
    try {
      const res = await fetch('/api/v1/notifications/channels/test', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(channel),
      })
      const v = await res.json()
      setState(v.ok ? 'ok' : 'fail')
      setMsg(v.message || '')
    } catch (e) {
      setState('fail'); setMsg(String(e))
    }
    setTimeout(() => setState('idle'), 4000)
  }
  return (
    <span class="inline-flex items-center gap-2">
      <button class="vz-btn vz-btn-ghost" disabled={state === 'busy'} onClick={test}>
        {state === 'busy' ? 'Sending…' : 'Test'}
      </button>
      {state === 'ok' && <span class="text-xs" style={{ color: 'var(--vz-good-text)' }}>Sent</span>}
      {state === 'fail' && <span class="text-xs" style={{ color: 'var(--vz-crit)' }} title={msg}>Failed</span>}
    </span>
  )
}

function ChannelModal({ channel, onClose, onSaved }: {
  channel: NotificationChannelDto | null
  onClose: () => void
  onSaved: () => void
}) {
  const [name, setName] = useState(channel?.name ?? '')
  const [kind, setKind] = useState<NotificationChannelKind>(channel?.kind ?? 'ntfy')
  // New ntfy channels start with an unguessable topic (see randomTopic)
  const [config, setConfig] = useState<{ [k: string]: string }>(channel?.config ?? { topic: randomTopic() })
  const [enabled, setEnabled] = useState(channel?.enabled ?? true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function save() {
    for (const [key, label, , required] of CHANNEL_FIELDS[kind]) {
      if (required && !(config[key] || '').trim()) { setError(`${label} is required`); return }
    }
    setSaving(true); setError(null)
    const body: NotificationChannelDto = {
      id: channel?.id ?? slug(name),
      name, kind, config, enabled,
    }
    const res = await fetch('/api/v1/notifications/channels', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    })
    setSaving(false)
    if (res.ok) onSaved(); else setError(await res.text())
  }

  return (
    <div class="fixed inset-0 flex items-center justify-center p-4" style={{ background: 'rgba(0,0,0,0.4)' }}>
      <div class="vz-card w-[520px] max-w-full">
        <div class="px-4 py-3 font-semibold" style={{ borderBottom: '1px solid var(--vz-border)', color: 'var(--vz-ink)' }}>
          {channel ? `Edit Channel — ${channel.name}` : 'Add Channel'}
        </div>
        <div class="p-4 space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <Field label="Name">
              <input class="vz-input" value={name} onInput={(e: any) => setName(e.target.value)} />
            </Field>
            <Field label="Type">
              <select class="vz-input" value={kind} onChange={(e: any) => setKind(e.target.value)}>
                {Object.entries(KIND_LABELS).map(([k, label]) => <option value={k}>{label}</option>)}
              </select>
            </Field>
            {CHANNEL_FIELDS[kind].map(([key, label, placeholder]) => (
              <Field label={label}>
                <input
                  class="vz-input"
                  type={key === 'password' ? 'password' : 'text'}
                  placeholder={placeholder}
                  value={config[key] ?? ''}
                  onInput={(e: any) => setConfig(p => ({ ...p, [key]: e.target.value }))}
                />
              </Field>
            ))}
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
          <button class="vz-btn vz-btn-primary" disabled={saving || !name.trim()} onClick={save}>Save</button>
        </div>
      </div>
    </div>
  )
}

function RuleModal({ rule, channels, onClose, onSaved }: {
  rule: NotificationRuleDto | null
  channels: NotificationChannelDto[]
  onClose: () => void
  onSaved: () => void
}) {
  const { devices } = useDevices()
  const [name, setName] = useState(rule?.name ?? '')
  const [event, setEvent] = useState<NotificationEvent>(rule?.event ?? 'gridState')
  const [deviceId, setDeviceId] = useState<string>(rule?.deviceId ?? '')
  const [params, setParams] = useState<{ [k: string]: number }>(rule?.params ?? {})
  const [channelIds, setChannelIds] = useState<string[]>(rule?.channelIds ?? [])
  const [enabled, setEnabled] = useState(rule?.enabled ?? true)
  const [cooldown, setCooldown] = useState(rule?.cooldownSeconds ?? 300)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  function toggleChannel(id: string, on: boolean) {
    setChannelIds(p => (on ? [...new Set([...p, id])] : p.filter(x => x !== id)))
  }

  async function save() {
    if (channelIds.length === 0) { setError('Pick at least one channel'); return }
    setSaving(true); setError(null)
    const body: NotificationRuleDto = {
      id: rule?.id ?? slug(name),
      name,
      event,
      deviceId: deviceId || null,
      params,
      channelIds,
      enabled,
      cooldownSeconds: cooldown,
    }
    const res = await fetch('/api/v1/notifications/rules', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    })
    setSaving(false)
    if (res.ok) onSaved(); else setError(await res.text())
  }

  return (
    <div class="fixed inset-0 flex items-center justify-center p-4" style={{ background: 'rgba(0,0,0,0.4)' }}>
      <div class="vz-card w-[520px] max-w-full">
        <div class="px-4 py-3 font-semibold" style={{ borderBottom: '1px solid var(--vz-border)', color: 'var(--vz-ink)' }}>
          {rule ? `Edit Rule — ${rule.name}` : 'Add Rule'}
        </div>
        <div class="p-4 space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <Field label="Name">
              <input class="vz-input" value={name} onInput={(e: any) => setName(e.target.value)} />
            </Field>
            <Field label="Event">
              <select class="vz-input" value={event} onChange={(e: any) => { setEvent(e.target.value); setParams({}) }}>
                {Object.entries(EVENT_LABELS).map(([k, label]) => <option value={k}>{label}</option>)}
              </select>
            </Field>
            <Field label="Device">
              <select class="vz-input" value={deviceId} onChange={(e: any) => setDeviceId(e.target.value)}>
                <option value="">Any device</option>
                {(devices || []).map(d => <option value={d.id}>{d.name}</option>)}
              </select>
            </Field>
            <Field label="Cooldown (s)">
              <input type="number" class="vz-input" value={cooldown} onInput={(e: any) => setCooldown(parseInt(e.target.value || '0'))} />
            </Field>
            {EVENT_PARAMS[event].map(([key, label, dflt]) => (
              <Field label={label}>
                <input
                  type="number"
                  class="vz-input"
                  value={params[key] ?? dflt}
                  onInput={(e: any) => setParams(p => ({ ...p, [key]: parseFloat(e.target.value || '0') }))}
                />
              </Field>
            ))}
          </div>
          <Field label="Send to">
            <div class="space-y-1">
              {channels.map(c => (
                <label class="flex items-center gap-2 text-sm" style={{ color: 'var(--vz-ink-2)' }}>
                  <input
                    type="checkbox"
                    checked={channelIds.includes(c.id)}
                    onChange={(e: any) => toggleChannel(c.id, e.target.checked)}
                  />
                  {c.name} <span style={{ color: 'var(--vz-ink-3)' }}>({KIND_LABELS[c.kind]})</span>
                </label>
              ))}
            </div>
          </Field>
          <label class="inline-flex items-center gap-2 text-sm" style={{ color: 'var(--vz-ink-2)' }}>
            <input type="checkbox" checked={enabled} onChange={(e: any) => setEnabled(e.target.checked)} /> Enabled
          </label>
          {error && <div class="text-sm" style={{ color: 'var(--vz-crit)' }}>{error}</div>}
        </div>
        <div class="px-4 py-3 flex justify-end gap-2" style={{ borderTop: '1px solid var(--vz-border)' }}>
          <button class="vz-btn vz-btn-ghost" onClick={onClose}>Cancel</button>
          <button class="vz-btn vz-btn-primary" disabled={saving || !name.trim()} onClick={save}>Save</button>
        </div>
      </div>
    </div>
  )
}
