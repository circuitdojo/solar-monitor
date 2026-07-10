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

// Common page frame: same width, padding, and header row (page-specific
// content left, nav tabs right) on every page.
export function PageShell({ header, children }: { header: any; children: any }) {
  return (
    <div class="p-4 md:p-6 space-y-4 max-w-6xl mx-auto">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <div class="flex items-center gap-3 flex-wrap">{header}</div>
        <Nav />
      </div>
      {children}
    </div>
  )
}

export function PageTitle({ children }: { children: any }) {
  return <h1 class="text-xl font-semibold" style={{ color: 'var(--vz-ink)' }}>{children}</h1>
}
