import type { ReactNode } from 'react'
import { AlertTriangle, RotateCw, type LucideIcon } from 'lucide-react'

import { cn } from '../shared/lib/cn'
import { Button } from '../shared/ui/button'

type HazardTone = 'warning' | 'error'

const TONE_HEADER: Record<HazardTone, string> = {
  error: 'hazard-error border-destructive/25 bg-destructive/[0.06]',
  warning: 'hazard-warning border-amber-500/25 bg-amber-500/[0.06]',
}

const TONE_ICON: Record<HazardTone, string> = {
  error: 'text-destructive',
  warning: 'text-amber-600 dark:text-amber-400',
}

const TONE_TITLE: Record<HazardTone, string> = {
  error: 'text-destructive',
  warning: 'text-amber-700 dark:text-orange-300',
}

const TONE_BORDER: Record<HazardTone, string> = {
  error: 'border-destructive/25',
  warning: 'border-amber-500/25',
}

const TONE_BODY: Record<HazardTone, string> = {
  error: 'bg-destructive/[0.03]',
  warning: 'bg-amber-500/[0.03]',
}

/**
 * The caution-tape header strip on its own — diagonal hazard stripes confined
 * to a single band with a tinted icon + title. The stripes live HERE only; the
 * surface below stays clean. Reuse this when you need the band without
 * `HazardPanel`'s border/body wrapper.
 */
export function HazardHeader({
  tone,
  title,
  icon: Icon = AlertTriangle,
  action,
  className,
}: {
  tone: HazardTone
  title: string
  icon?: LucideIcon
  action?: ReactNode
  className?: string
}) {
  return (
    <div
      className={cn(
        'flex items-center gap-2.5 border-b px-4 py-2.5 sm:px-5',
        TONE_HEADER[tone],
        className,
      )}
    >
      <Icon className={cn('size-4 shrink-0', TONE_ICON[tone])} aria-hidden />
      <span
        className={cn(
          'font-display text-sm font-semibold tracking-[-0.01em]',
          TONE_TITLE[tone],
        )}
      >
        {title}
      </span>
      {action && <span className="ml-auto flex shrink-0 items-center">{action}</span>}
    </div>
  )
}

/**
 * The caution-tape band turned on its side — a thin vertical LEFT rail, the
 * row-shaped analog of `HazardHeader`. Use on list/table rows where a top strip
 * doesn't fit: the diagonal stripes live in this narrow left band and the row
 * body to its right stays clean (so it needs NO `hazard-text` plates). Render
 * it as the first flex child of a stretched row.
 */
export function HazardRail({
  tone,
  className,
}: {
  tone: HazardTone
  className?: string
}) {
  return (
    <div
      aria-hidden
      className={cn('w-2.5 shrink-0 self-stretch border-r', TONE_HEADER[tone], className)}
    />
  )
}

interface HazardPanelProps {
  tone: HazardTone
  title: string
  icon?: LucideIcon
  /** Optional element pinned to the right of the header strip. */
  headerAction?: ReactNode
  /** Clean body content — never striped. */
  children?: ReactNode
  className?: string
  bodyClassName?: string
}

/**
 * A bordered panel whose ONLY hazard styling is a striped header strip; the
 * body below is a clean, faintly-tinted surface. This is the single shape for
 * both error states (`tone="error"`, red stripes) and danger/warning zones
 * (`tone="warning"`, amber stripes) — the diagonal caution tape is restricted
 * to the header so body copy stays perfectly legible.
 */
export function HazardPanel({
  tone,
  title,
  icon,
  headerAction,
  children,
  className,
  bodyClassName,
}: HazardPanelProps) {
  return (
    <div className={cn('overflow-hidden rounded-lg border', TONE_BORDER[tone], className)}>
      <HazardHeader tone={tone} title={title} icon={icon} action={headerAction} />
      {children != null && (
        <div className={cn('px-4 py-3.5 sm:px-5', TONE_BODY[tone], bodyClassName)}>
          {children}
        </div>
      )}
    </div>
  )
}

interface ErrorPanelProps {
  /** Strip headline. Defaults to "Couldn't load". */
  title?: string
  /** Detail message shown in the clean body. */
  message?: ReactNode
  /** When provided, renders a Retry button. */
  onRetry?: () => void
  retryLabel?: string
  className?: string
}

/**
 * The standard load-failure surface: an error-toned `HazardPanel` with the
 * detail message and an optional Retry button in the body. Use this anywhere a
 * fetch can fail and you want an inline, recoverable error rather than a toast.
 */
export function ErrorPanel({
  title = "Couldn't load",
  message,
  onRetry,
  retryLabel = 'Retry',
  className,
}: ErrorPanelProps) {
  return (
    <HazardPanel tone="error" title={title} className={className}>
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        {message != null && (
          <p className="min-w-0 text-sm leading-relaxed text-foreground/80">{message}</p>
        )}
        {onRetry && (
          <Button
            variant="outline"
            size="sm"
            onClick={onRetry}
            className="shrink-0 self-start border-destructive/30 text-destructive hover:bg-destructive/10 hover:text-destructive sm:self-center"
          >
            <RotateCw className="size-3.5" />
            {retryLabel}
          </Button>
        )}
      </div>
    </HazardPanel>
  )
}
