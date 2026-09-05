import { Component, type ErrorInfo, type ReactNode } from 'react'
import { Button } from '../shared/ui/button'
import { HazardHeader } from './HazardPanel'

interface ErrorBoundaryProps {
  children: ReactNode
  fallback?: ReactNode
  /**
   * When this value changes the boundary resets its error state. Pass
   * `location.pathname` so navigating to a new route clears a caught error
   * rather than keeping the fallback UI on every subsequent route.
   */
  resetKey?: string
}

interface ErrorBoundaryState {
  error: Error | null
}

/**
 * App-level error boundary around the protected shell. An uncaught render
 * error in any page would otherwise blank the whole SPA; here it shows a clean
 * recovery card instead. Class component because error boundaries have no hook
 * equivalent.
 *
 * Pass `resetKey={location.pathname}` (via a wrapper that calls `useLocation`)
 * so navigating to a new route resets the caught error state instead of
 * persisting the fallback UI across every subsequent route.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props)
    this.state = { error: null }
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('Uncaught error:', error, info.componentStack)
  }

  componentDidUpdate(prevProps: ErrorBoundaryProps): void {
    if (
      this.state.error !== null &&
      prevProps.resetKey !== this.props.resetKey
    ) {
      this.setState({ error: null })
    }
  }

  render(): ReactNode {
    const { error } = this.state
    if (!error) return this.props.children
    if (this.props.fallback !== undefined) return this.props.fallback

    return (
      <div className="flex min-h-[60vh] w-full flex-1 items-center justify-center p-6">
        <div className="w-full max-w-md overflow-hidden rounded-xl border border-border/60 bg-card shadow-sm">
          <HazardHeader tone="error" title="Something went wrong" />
          <div className="p-8 text-center">
            <p className="break-words text-sm text-muted-foreground">{error.message}</p>
            <Button className="mt-6" onClick={() => window.location.reload()}>
              Reload
            </Button>
          </div>
        </div>
      </div>
    )
  }
}
