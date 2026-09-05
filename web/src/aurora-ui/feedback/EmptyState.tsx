import type { LucideIcon } from 'lucide-react'
import type { ReactNode } from 'react'

import { cn } from '../shared/lib/cn'

type EmptyStateTone = 'primary' | 'muted' | 'emerald'

interface EmptyStateProps {
  /** Icon shown in the medallion. */
  icon: LucideIcon
  /** Headline — short, in the display face. */
  title: string
  /** Supporting copy. Keep it to a sentence or two. */
  description?: ReactNode
  /**
   * Call-to-action row, rendered centred below the copy. Pass one or more
   * `<Button>`s; they wrap on narrow screens.
   */
  action?: ReactNode
  /**
   * `card` (default) draws a dashed, faintly-tinted panel that stands on its own.
   * `bare` drops the border/background for use INSIDE an existing card or list.
   */
  variant?: 'card' | 'bare'
  /** Medallion accent. Defaults to the brand primary. */
  tone?: EmptyStateTone
  /** `sm` tightens the vertical rhythm for dense surfaces (sidebars, tab panes). */
  size?: 'sm' | 'default'
  className?: string
}

const TONE_MEDALLION: Record<EmptyStateTone, string> = {
  primary: 'bg-primary/10 text-primary ring-primary/15',
  muted: 'bg-muted text-muted-foreground ring-border/70',
  emerald:
    'bg-emerald-500/10 text-emerald-600 ring-emerald-500/20 dark:text-emerald-400 dark:ring-emerald-500/25',
}

const TONE_GLOW: Record<EmptyStateTone, string> = {
  primary: 'bg-primary/25',
  muted: 'bg-foreground/10',
  emerald: 'bg-emerald-500/25',
}

/**
 * The single, polished empty state used across the app. A glowing icon
 * medallion over a dashed panel, a display-face headline, muted supporting
 * copy, and an optional CTA row — so "there's nothing here yet" reads as
 * intentional and inviting rather than broken.
 *
 * For error/danger surfaces use `HazardPanel`/`ErrorPanel` instead — empty
 * states are neutral and never wear the hazard stripes.
 */
export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  variant = 'card',
  tone = 'primary',
  size = 'default',
  className,
}: EmptyStateProps) {
  const dense = size === 'sm'
  return (
    <div
      className={cn(
        'flex flex-col items-center text-center',
        dense ? 'gap-3 px-5 py-10' : 'gap-4 px-6 py-14',
        variant === 'card' &&
          'rounded-2xl border border-dashed border-border/70 bg-card/40',
        className,
      )}
    >
      <div className="relative">
        {/* Soft radial glow behind the medallion for depth. */}
        <span
          aria-hidden
          className={cn(
            'absolute inset-0 -z-10 rounded-full blur-2xl',
            TONE_GLOW[tone],
          )}
        />
        <span
          className={cn(
            'inline-flex items-center justify-center rounded-2xl ring-1 ring-inset',
            dense ? 'size-12' : 'size-14',
            TONE_MEDALLION[tone],
          )}
        >
          <Icon className={dense ? 'size-6' : 'size-7'} aria-hidden />
        </span>
      </div>

      <div className="space-y-1.5">
        <p className="font-display text-base font-semibold tracking-[-0.01em]">
          {title}
        </p>
        {description && (
          <p className="mx-auto max-w-sm text-sm leading-relaxed text-muted-foreground">
            {description}
          </p>
        )}
      </div>

      {action && (
        <div className="flex flex-wrap items-center justify-center gap-2 pt-1">
          {action}
        </div>
      )}
    </div>
  )
}
