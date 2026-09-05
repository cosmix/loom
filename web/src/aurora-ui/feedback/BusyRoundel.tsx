import type { CSSProperties } from 'react'
import { useEffect, useState } from 'react'

import { cn } from '../shared/lib/cn'

/**
 * How long the disc keeps spinning after `busy` last reported work.
 *
 * A continuous wave of async work (fetches draining and refilling, jobs
 * finishing and re-queuing) can legitimately read as idle for a frame or two
 * between one drain and the next admission; without this trailing hold the
 * disc stutters on/off mid-wave. Short enough that it never meaningfully
 * claims busyness after work has genuinely finished.
 */
export const BUSY_TRAILING_HOLD_MS = 300

export interface BusyRoundelProps {
  /** Whether work is outstanding. Drives the spin and the live-region text. */
  busy: boolean
  /** How long the disc keeps spinning after `busy` goes false. Defaults to 300 ms. */
  trailingHoldMs?: number
  /** Disc diameter as a CSS length; a number is treated as px. Defaults to the stylesheet's `1rem`. */
  size?: number | string
  /** Live-region text while spinning. Defaults to "Loading data". */
  busyLabel?: string
  /** Live-region text at rest. Defaults to "Idle". */
  idleLabel?: string
  /** Extra classes on the wrapper `<span>`. */
  className?: string
}

/**
 * The classic OS busy disc — a circle quartered by a cross, alternating black
 * and white, rotating while `busy` is true. Pure CSS (one `conic-gradient`
 * for the quadrants, with no drawn cross — the quadrant boundaries are the
 * cross — under a static gloss overlay): the disc itself only carries a
 * `data-busy` attribute — React owns WHEN it spins, CSS owns the animation
 * (see `busy-roundel.css`).
 *
 * This is a single GLOBAL busy instrument: mount one per app and feed it a
 * `busy` prop derived from whatever work you want it to represent (in-flight
 * requests, a job queue, …). It carries its own trailing hold so a bursty or
 * continuous stream of work doesn't flicker the disc between frames — a
 * continuous wave can legitimately read as idle for a frame or two between
 * one drain and the next admission, and without the hold the disc stutters
 * mid-wave.
 *
 * The `role="status"`/`aria-live="polite"` region lives on a SEPARATE,
 * visually-hidden text node — never on the spinning graphic itself, which
 * stays `aria-hidden` — so a screen reader gets one honest announcement
 * instead of being read the rotating decoration. The announcement follows
 * the HELD state, so a mid-wave frame of idleness cannot chatter
 * "busy/idle/busy" at the reader either.
 *
 * Requires `busy-roundel.css` — see the folder README.
 */
export function BusyRoundel({
  busy,
  trailingHoldMs = BUSY_TRAILING_HOLD_MS,
  size,
  busyLabel = 'Loading data',
  idleLabel = 'Idle',
  className,
}: BusyRoundelProps) {
  const [heldIdle, setHeldIdle] = useState(!busy)
  // Rising edge adjusts state DURING render (React's sanctioned derived-state
  // pattern) so the disc starts the same frame work begins; only the falling
  // edge goes through the trailing-hold timer below.
  if (busy && heldIdle) setHeldIdle(false)
  const spinning = busy || !heldIdle

  useEffect(() => {
    if (busy) return undefined
    const timer = setTimeout(() => setHeldIdle(true), trailingHoldMs)
    return () => clearTimeout(timer)
  }, [busy, trailingHoldMs])

  const style: CSSProperties | undefined =
    size === undefined
      ? undefined
      : ({ '--aurora-roundel-size': typeof size === 'number' ? `${size}px` : size } as CSSProperties)

  return (
    <span className={cn('aurora-roundel-wrap', className)} style={style}>
      <span className="aurora-roundel" data-busy={spinning ? 'true' : undefined} aria-hidden="true" />
      {/* The gloss is a sibling of the disc, not a child of it: the disc
          rotates, and a highlight that turns with the quadrants stops reading
          as light. Following the disc in document order is required — the
          CSS dims it with the disc through `[data-busy] ~`. */}
      <span className="aurora-roundel-gloss" aria-hidden="true" />
      <span role="status" aria-live="polite" className="sr-only">
        {spinning ? busyLabel : idleLabel}
      </span>
    </span>
  )
}
