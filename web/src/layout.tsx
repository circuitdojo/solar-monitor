import { useEffect, useState } from 'preact/hooks'
import { Link, useLocation } from 'wouter'

const TABS: [string, string][] = [
  ['/', 'Dashboard'],
  ['/devices', 'Devices'],
  ['/settings', 'Settings'],
  ['/notifications', 'Notifications'],
]

export function Nav() {
  const [loc] = useLocation()
  return (
    <nav class="flex items-center gap-1">
      {TABS.map(([href, label]) => {
        const active = loc === href
        return (
          <Link href={href}>
            <a
              class="text-sm px-2.5 py-1 rounded-md hover:underline"
              style={active
                ? {
                    color: 'var(--vz-ink)',
                    background: 'var(--vz-surface)',
                    border: '1px solid var(--vz-border)',
                    fontWeight: 500,
                    textDecoration: 'none',
                  }
                : { color: 'var(--vz-load)' }}
            >
              {label}
            </a>
          </Link>
        )
      })}
    </nav>
  )
}

function useVersion() {
  const [version, setVersion] = useState<string | null>(null)
  useEffect(() => {
    let cancelled = false
    fetch('/api/v1/health')
      .then(r => r.json())
      .then(d => { if (!cancelled && d.version) setVersion(d.version) })
      .catch(() => {})
    return () => { cancelled = true }
  }, [])
  return version
}

// Common page frame: same width, padding, header row (page-specific content
// left, nav tabs right), and version footer on every page.
export function PageShell({ header, children }: { header: any; children: any }) {
  const version = useVersion()
  return (
    <div class="p-4 md:p-6 space-y-4 max-w-6xl mx-auto">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <div class="flex items-center gap-3 flex-wrap">{header}</div>
        <Nav />
      </div>
      {children}
      <footer class="pt-4 pb-1 text-center text-xs" style={{ color: 'var(--vz-ink-3)' }}>
        Solar Monitor{version ? ` v${version}` : ''} ·{' '}
        <a class="hover:underline" href="https://github.com/circuitdojo/solar-monitor" target="_blank" rel="noreferrer">
          GitHub
        </a>
      </footer>
    </div>
  )
}

export function PageTitle({ children }: { children: any }) {
  return <h1 class="text-xl font-semibold" style={{ color: 'var(--vz-ink)' }}>{children}</h1>
}
