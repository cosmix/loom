---
name: loom-react
description: Modern React development patterns. Use for building React 19+ applications with components, hooks, Jotai state, React Router v7, server components, accessibility, performance optimization, and testing. Covers client-side routing, composition, and async data loading.
triggers:
  - react
  - jsx
  - tsx
  - component
  - hook
  - useState
  - useEffect
  - useContext
  - useReducer
  - useMemo
  - useCallback
  - useRef
  - props
  - state
  - render
  - virtual DOM
  - reconciliation
  - single page application
  - spa
  - react-router
  - jotai
  - vite
  - bun
  - Next.js
  - Remix
  - client-side routing
  - server components
  - accessibility
  - a11y
  - ARIA
  - performance
  - code splitting
  - lazy loading
  - Suspense
  - error boundaries
  - form validation
  - UI components
  - design system
  - composition patterns
---

# React SPA Development

## Overview

Client-side React 19+ SPAs. Stack: **React Router v7** (routing/loaders), **Jotai** (atomic global state), **Vite** (build), **Bun** (package manager/runtime). NOT for SSR frameworks (Next.js/Remix) — those are out of scope.

The single densest section is **Expert Practices** at the end — read it first if you know React basics. The middle sections are reference implementations.

## React 19 Features

### Actions and useActionState

React 19 introduces Actions for handling async state transitions:

```typescript
import { useActionState } from 'react'

interface FormState {
  message: string
  error?: string
}

async function updateProfile(previousState: FormState, formData: FormData) {
  const name = formData.get('name') as string

  try {
    await fetch('/api/profile', {
      method: 'POST',
      body: JSON.stringify({ name }),
    })
    return { message: 'Profile updated successfully' }
  } catch (error) {
    return { message: '', error: 'Update failed' }
  }
}

export function ProfileForm() {
  const [state, formAction, isPending] = useActionState(updateProfile, { message: '' })

  return (
    <form action={formAction}>
      <input type="text" name="name" disabled={isPending} />
      <button type="submit" disabled={isPending}>
        {isPending ? 'Updating...' : 'Update Profile'}
      </button>
      {state.error && <p className="error">{state.error}</p>}
      {state.message && <p className="success">{state.message}</p>}
    </form>
  )
}
```

### useOptimistic for Instant UI Updates

```typescript
import { useOptimistic, useState } from 'react'

interface Todo {
  id: string
  title: string
  completed: boolean
}

export function TodoList({ todos }: { todos: Todo[] }) {
  const [optimisticTodos, addOptimisticTodo] = useOptimistic(
    todos,
    (state, newTodo: Todo) => [...state, newTodo]
  )

  async function addTodo(formData: FormData) {
    const title = formData.get('title') as string
    const tempTodo = { id: crypto.randomUUID(), title, completed: false }

    addOptimisticTodo(tempTodo)

    await fetch('/api/todos', {
      method: 'POST',
      body: JSON.stringify({ title }),
    })
  }

  return (
    <div>
      <ul>
        {optimisticTodos.map((todo) => (
          <li key={todo.id}>{todo.title}</li>
        ))}
      </ul>
      <form action={addTodo}>
        <input type="text" name="title" />
        <button type="submit">Add Todo</button>
      </form>
    </div>
  )
}
```

### use() for Reading Promises and Context

```typescript
import { use, Suspense } from 'react'
import { useLoaderData } from 'react-router'

interface User {
  id: string
  name: string
}

function UserProfile({ userPromise }: { userPromise: Promise<User> }) {
  // use() unwraps the promise. It cannot be wrapped in try/catch —
  // a rejected promise surfaces at the nearest Error Boundary.
  const user = use(userPromise)

  return <div>{user.name}</div>
}

// CRITICAL: the Promise must be created OUTSIDE the render cycle. Promises
// created in client components are recreated on every render, so passing an
// inline fetchUser(userId) to use() re-suspends and re-fetches forever.
// In this React Router v7 SPA stack a route loader is the idiomatic stable
// source (one promise per navigation).
export function UserContainer() {
  const userPromise = useLoaderData() as Promise<User>

  return (
    <Suspense fallback={<div>Loading user...</div>}>
      <UserProfile userPromise={userPromise} />
    </Suspense>
  )
}
```

### Document Metadata

React 19 hoists `<title>`/`<meta>`/`<link>` rendered anywhere in the tree into `<head>` — no helper library needed. Just render them inside the component (e.g. a route page).

**Server/Client Components:** N/A here — a pure SPA has no server, so every component is a client component with full access to browser APIs, hooks, and event handlers. `'use client'`/RSC only matter under Next.js/Remix (out of scope).

## UI Component Patterns

### Design System Foundation

```typescript
// src/components/ui/Button.tsx
import { ComponentPropsWithoutRef } from 'react'
import { cva, type VariantProps } from 'class-variance-authority'

const buttonVariants = cva(
  'inline-flex items-center justify-center rounded-md font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        default: 'bg-primary text-white hover:bg-primary/90',
        secondary: 'bg-secondary text-white hover:bg-secondary/90',
        outline: 'border border-gray-300 bg-transparent hover:bg-gray-100',
        ghost: 'hover:bg-gray-100',
        danger: 'bg-red-600 text-white hover:bg-red-700',
      },
      size: {
        sm: 'h-8 px-3 text-sm',
        md: 'h-10 px-4',
        lg: 'h-12 px-6 text-lg',
        icon: 'h-10 w-10',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'md',
    },
  }
)

export interface ButtonProps
  extends ComponentPropsWithoutRef<'button'>,
    VariantProps<typeof buttonVariants> {
  isLoading?: boolean
  ref?: React.Ref<HTMLButtonElement>
}

// React 19: ref is a plain prop — no forwardRef wrapper, no displayName needed.
export function Button({
  ref,
  className,
  variant,
  size,
  isLoading,
  children,
  ...props
}: ButtonProps) {
  return (
    <button
      ref={ref}
      className={buttonVariants({ variant, size, className })}
      disabled={isLoading || props.disabled}
      {...props}
    >
      {isLoading ? (
        <>
          <svg className="animate-spin -ml-1 mr-2 h-4 w-4" />
          Loading...
        </>
      ) : (
        children
      )}
    </button>
  )
}
```

### Composition (Card slots)

Ship a family of thin primitives that forward `ref` (a plain prop in React 19) and `className`. Callers compose them; no prop explosion.

```typescript
type DivProps = ComponentPropsWithoutRef<'div'> & { ref?: React.Ref<HTMLDivElement> }

export function Card({ ref, className, ...props }: DivProps) {
  return <div ref={ref} className={`rounded-lg border bg-white shadow-sm ${className}`} {...props} />
}
export function CardHeader({ ref, className, ...props }: DivProps) {
  return <div ref={ref} className={`p-6 ${className}`} {...props} />
}
export function CardContent({ ref, className, ...props }: DivProps) {
  return <div ref={ref} className={`p-6 pt-0 ${className}`} {...props} />
}
// <Card><CardHeader>…</CardHeader><CardContent>…</CardContent></Card>
```

### Polymorphic Components

```typescript
// src/components/ui/Text.tsx
import { ElementType, ComponentPropsWithoutRef } from 'react'

type TextProps<E extends ElementType> = {
  as?: E
  variant?: 'h1' | 'h2' | 'h3' | 'body' | 'small'
} & ComponentPropsWithoutRef<E>

export function Text<E extends ElementType = 'p'>({
  as,
  variant = 'body',
  className,
  ...props
}: TextProps<E>) {
  const Component = as || 'p'

  const variantClasses = {
    h1: 'text-4xl font-bold',
    h2: 'text-3xl font-semibold',
    h3: 'text-2xl font-semibold',
    body: 'text-base',
    small: 'text-sm text-gray-600',
  }

  return (
    <Component
      className={`${variantClasses[variant]} ${className || ''}`}
      {...props}
    />
  )
}

// Usage - flexible element types
<Text variant="h1">Heading</Text>
<Text as="h1" variant="h1">Heading with h1 tag</Text>
<Text as="span" variant="small">Small text in span</Text>
```

### Render Props / Function-as-Children

Generic state-branching component. `children: (data: T) => ReactNode`. In this stack prefer Suspense + async atoms/loaders for data; render props remain useful for non-Suspense state machines.

```typescript
export function DataLoader<T>({ data, isLoading, error, children }: {
  data: T | null; isLoading: boolean; error: Error | null; children: (data: T) => ReactNode
}) {
  if (isLoading) return <div>Loading…</div>
  if (error) return <div>Error: {error.message}</div>
  if (!data) return null
  return <>{children(data)}</>
}
// <DataLoader {...useFetch<User[]>('/api/users')}>{(u) => …}</DataLoader>
```

## Project Setup

### Initial Setup with Bun and Vite

```bash
# Create new React app with Vite template
bun create vite my-app --template react-ts
cd my-app

# Install dependencies
bun install

# Add React Router and Jotai
bun add react-router jotai

# Add development dependencies
bun add -D @types/react @types/react-dom

# Start development server
bun run dev
```

### Vite Configuration

```typescript
// vite.config.ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

export default defineConfig({
  // Fast Refresh is always on; @vitejs/plugin-react removed the `fastRefresh`
  // option in v4. Pass the options object only for real settings,
  // e.g. react({ babel: { plugins: [...] } }).
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@components": path.resolve(__dirname, "./src/components"),
      "@hooks": path.resolve(__dirname, "./src/hooks"),
      "@store": path.resolve(__dirname, "./src/store"),
      "@utils": path.resolve(__dirname, "./src/utils"),
    },
  },
  server: {
    port: 3000,
    open: true,
  },
  build: {
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks: {
          "react-vendor": ["react", "react-dom"],
          router: ["react-router"],
          state: ["jotai"],
        },
      },
    },
  },
});
```

### TypeScript Configuration

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,

    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",

    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "noUncheckedIndexedAccess": true,

    "baseUrl": ".",
    "paths": {
      "@/*": ["./src/*"],
      "@components/*": ["./src/components/*"],
      "@hooks/*": ["./src/hooks/*"],
      "@store/*": ["./src/store/*"],
      "@utils/*": ["./src/utils/*"]
    }
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

## React Router v7 Patterns

### Router Setup with createBrowserRouter

```typescript
// src/main.tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider, createBrowserRouter } from 'react-router'
import './index.css'

// Import route components
import { RootLayout } from './layouts/RootLayout'
import { HomePage } from './pages/HomePage'
import { AboutPage } from './pages/AboutPage'
import { UsersPage } from './pages/users/UsersPage'
import { UserDetailPage } from './pages/users/UserDetailPage'
import { ErrorPage } from './pages/ErrorPage'
import { NotFoundPage } from './pages/NotFoundPage'

// Create router with type-safe route definitions
const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    errorElement: <ErrorPage />,
    children: [
      {
        index: true,
        element: <HomePage />,
      },
      {
        path: 'about',
        element: <AboutPage />,
      },
      {
        path: 'users',
        children: [
          {
            index: true,
            element: <UsersPage />,
            loader: async () => {
              // Data loading for users list
              const response = await fetch('/api/users')
              return response.json()
            },
          },
          {
            path: ':userId',
            element: <UserDetailPage />,
            loader: async ({ params }) => {
              // Data loading for specific user
              const response = await fetch(`/api/users/${params.userId}`)
              if (!response.ok) {
                throw new Response('User not found', { status: 404 })
              }
              return response.json()
            },
          },
        ],
      },
      {
        path: '*',
        element: <NotFoundPage />,
      },
    ],
  },
])

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>
)
```

### Root Layout with Outlet

The layout renders shared chrome (`<nav>`) plus `<Outlet />` for the matched child route; `useNavigation().state === 'loading'` drives a global pending indicator.

```typescript
export function RootLayout() {
  const isNavigating = useNavigation().state === 'loading'
  return (
    <>
      <nav><Link to="/">Home</Link><Link to="/users">Users</Link></nav>
      <main>{isNavigating && <div className="loading-bar" />}<Outlet /></main>
    </>
  )
}
```

### Data Loading with Loaders

React Router v7 framework mode generates a per-route `+types/<route>.d.ts` via
`react-router typegen`, exposing a `Route` namespace (`LoaderArgs`, `ActionArgs`,
`ComponentProps`). Consume loader data through the typed `loaderData` prop — NOT
`useLoaderData() as SomeType`, an unsafe cast that hides divergence between the
loader's real return and the component's expectation.

```typescript
// src/pages/users/UsersPage.tsx
import { Link } from 'react-router'
import type { Route } from './+types/UsersPage'

interface User {
  id: string
  name: string
  email: string
}

export async function loader(): Promise<User[]> {
  return (await fetch('/api/users')).json()
}

export default function UsersPage({ loaderData }: Route.ComponentProps) {
  return (
    <div>
      <h1>Users</h1>
      <ul>
        {loaderData.map((user) => (
          <li key={user.id}>
            <Link to={`/users/${user.id}`}>
              {user.name} ({user.email})
            </Link>
          </li>
        ))}
      </ul>
    </div>
  )
}
```

Typegen setup: add `.react-router/` to `.gitignore`, set tsconfig `include` to
`.react-router/types/**/*`, set `compilerOptions.rootDirs` to
`[".", "./.react-router/types"]`, and run `react-router typegen && tsc`.

### Navigation Hooks

- `useNavigate()` → imperative nav: `navigate('/users/' + id)`, `navigate(-1)`, `navigate(path, { replace: true, state })`.
- `useSearchParams()` → `[params, setSearchParams]`; `params.get('filter')`, `setSearchParams({ filter })` (updates URL, drives derived state — do NOT mirror URL into `useState`).
- `useNavigation()` (from the router) → global `state === 'loading'` during transitions; drives loading bars.
- `useParams()` → typed route params; `useLoaderData()`/typed `loaderData` prop → loader result.

### Protected Routes Pattern

```typescript
// src/components/ProtectedRoute.tsx
import { Navigate, Outlet } from 'react-router'
import { useAtomValue } from 'jotai'
import { userAtom } from '@store/auth'

export function ProtectedRoute() {
  const user = useAtomValue(userAtom)

  if (!user) {
    return <Navigate to="/login" replace />
  }

  return <Outlet />
}

// Usage in router configuration
const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    children: [
      {
        path: 'dashboard',
        element: <ProtectedRoute />,
        children: [
          {
            index: true,
            element: <DashboardPage />,
          },
          {
            path: 'settings',
            element: <SettingsPage />,
          },
        ],
      },
    ],
  },
])
```

## Jotai State Management

⚠️ **Define every atom at module scope, never inside a component.** An atom is an identity/key, not a value — the store maps atom identity → state. An atom created in render is a brand-new key each render, so state never persists and subscribers thrash. For per-item/per-id atoms use `atomFamily` (memoizes by param at module scope), not `useMemo(() => atom(...))`.

Hooks: `useAtom` (read+write), `useAtomValue` (read), `useSetAtom` (write-only — subscriber does NOT re-render on value change; use for actions).

### Basic Atoms

```typescript
// src/store/counter.ts
import { atom } from 'jotai'

// Primitive atom
export const countAtom = atom(0)

// Read-only derived atom
export const doubledCountAtom = atom((get) => get(countAtom) * 2)

// Read-write derived atom
export const incrementAtom = atom(
  (get) => get(countAtom),
  (get, set) => set(countAtom, get(countAtom) + 1)
)

export const decrementAtom = atom(
  null,
  (get, set) => set(countAtom, get(countAtom) - 1)
)

// Usage in component
import { useAtom, useAtomValue, useSetAtom } from 'jotai'

export function Counter() {
  const [count, setCount] = useAtom(countAtom)
  const doubled = useAtomValue(doubledCountAtom)
  const increment = useSetAtom(incrementAtom)

  return (
    <div>
      <p>Count: {count}</p>
      <p>Doubled: {doubled}</p>
      <button onClick={increment}>Increment</button>
      <button onClick={() => setCount((c) => c - 1)}>Decrement</button>
    </div>
  )
}
```

### Async Atoms

An atom whose read fn returns a Promise integrates with Suspense automatically: `useAtomValue` unwraps it, and the nearest `<Suspense>` shows the fallback while pending, the nearest ErrorBoundary catches rejection. Add a "refresh trigger" atom as a dependency to force refetch.

```typescript
export const usersAtom = atom(async () => {
  const res = await fetch('/api/users')
  if (!res.ok) throw new Error('Failed to fetch users') // → ErrorBoundary
  return res.json() as Promise<User[]>
})
export const refreshUsersAtom = atom(0) // set() to bump; the atom below re-reads
export const refreshableUsersAtom = atom(async (get) => {
  get(refreshUsersAtom)
  return (await fetch('/api/users')).json() as Promise<User[]>
})
// Consumer: const users = useAtomValue(usersAtom) inside a <Suspense fallback={…}>.
```

### Atom Families

```typescript
// src/store/todos.ts
import { atom } from 'jotai'
import { atomFamily } from 'jotai/utils'

interface Todo {
  id: string
  title: string
  completed: boolean
}

// Base todos atom
export const todosAtom = atom<Todo[]>([])

// Atom family for individual todos
export const todoAtomFamily = atomFamily((id: string) =>
  atom(
    (get) => get(todosAtom).find((todo) => todo.id === id),
    (get, set, update: Partial<Todo>) => {
      const todos = get(todosAtom)
      const index = todos.findIndex((todo) => todo.id === id)
      if (index !== -1) {
        const newTodos = [...todos]
        newTodos[index] = { ...newTodos[index]!, ...update }
        set(todosAtom, newTodos)
      }
    }
  )
)

// Usage
function TodoItem({ id }: { id: string }) {
  const [todo, updateTodo] = useAtom(todoAtomFamily(id))

  if (!todo) return null

  return (
    <div>
      <input
        type="checkbox"
        checked={todo.completed}
        onChange={(e) => updateTodo({ completed: e.target.checked })}
      />
      <span>{todo.title}</span>
    </div>
  )
}
```

### Persistent Storage with atomWithStorage

```typescript
// src/store/auth.ts
import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";

interface User {
  id: string;
  name: string;
  email: string;
  token: string;
}

// Persists to localStorage automatically
export const userAtom = atomWithStorage<User | null>("user", null);

export const isAuthenticatedAtom = atom((get) => {
  const user = get(userAtom);
  return user !== null;
});

// Login action
export const loginAtom = atom(
  null,
  async (get, set, credentials: { email: string; password: string }) => {
    const response = await fetch("/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(credentials),
    });

    if (!response.ok) {
      throw new Error("Login failed");
    }

    const user = await response.json();
    set(userAtom, user);
    return user;
  },
);

// Logout action
export const logoutAtom = atom(null, (get, set) => {
  set(userAtom, null);
});
```

### Composition: base + derived + write-only actions

The idiom: one persisted/base atom, read-only derived atoms (`atom((get) => …)`) for computed views, and write-only action atoms (`atom(null, (get, set, arg) => …)`) that encapsulate mutations. Components read derived atoms and call actions — never duplicate derived data into separate state.

```typescript
export const cartItemsAtom = atomWithStorage<CartItem[]>('cart', [])
export const cartTotalAtom = atom((get) =>
  get(cartItemsAtom).reduce((s, i) => s + i.price * i.quantity, 0))

export const addToCartAtom = atom(null, (get, set, item: CartItem) => {
  const items = get(cartItemsAtom)
  const i = items.findIndex((x) => x.productId === item.productId)
  set(cartItemsAtom, i === -1
    ? [...items, item]
    : items.map((x, idx) => idx === i ? { ...x, quantity: x.quantity + item.quantity } : x))
})
```

## Component Patterns

### Custom Hooks

```typescript
// src/hooks/useDebounce.ts
import { useEffect, useState } from 'react'

export function useDebounce<T>(value: T, delay: number = 500): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value)

  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value)
    }, delay)

    return () => {
      clearTimeout(handler)
    }
  }, [value, delay])

  return debouncedValue
}
// const debounced = useDebounce(search, 300) — drive a derived query, not a setState-in-effect
```

For `localStorage`-backed state prefer Jotai `atomWithStorage` over a hand-rolled `useLocalStorage` — it handles serialization, cross-tab sync, and shared identity. Roll your own only for truly local, non-shared values.

```typescript
// src/hooks/useFetch.ts — abort on unmount / url change to avoid setState-after-unmount
import { useState, useEffect } from "react";

interface UseFetchResult<T> {
  data: T | null;
  error: Error | null;
  isLoading: boolean;
  refetch: () => void;
}

export function useFetch<T>(url: string): UseFetchResult<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [refetchIndex, setRefetchIndex] = useState(0);

  useEffect(() => {
    const controller = new AbortController();

    const fetchData = async () => {
      setIsLoading(true);
      setError(null);

      try {
        const response = await fetch(url, { signal: controller.signal });
        if (!response.ok) {
          throw new Error(`HTTP error! status: ${response.status}`);
        }
        const json = await response.json();
        setData(json);
      } catch (err) {
        if (err instanceof Error && err.name !== "AbortError") {
          setError(err);
        }
      } finally {
        setIsLoading(false);
      }
    };

    fetchData();

    return () => {
      controller.abort();
    };
  }, [url, refetchIndex]);

  const refetch = () => setRefetchIndex((i) => i + 1);

  return { data, error, isLoading, refetch };
}
```

### Compound Components Pattern

Share implicit state via Context between a parent and its named sub-components; hang children off the parent (`Tabs.Tab`). A `useTabs()` guard hook throws if used outside the provider — fail loud, not silently.

```typescript
const TabsContext = createContext<{ active: string; setActive: (id: string) => void } | undefined>(undefined)
const useTabs = () => {
  const c = useContext(TabsContext)
  if (!c) throw new Error('Tabs.* must be used within <Tabs>')
  return c
}

export function Tabs({ defaultTab, children }: { defaultTab: string; children: ReactNode }) {
  const [active, setActive] = useState(defaultTab)
  return <TabsContext value={{ active, setActive }}>{children}</TabsContext> // React 19: context is its own provider
}
function Tab({ id, children }: { id: string; children: ReactNode }) {
  const { active, setActive } = useTabs()
  return <button className={active === id ? 'active' : ''} onClick={() => setActive(id)}>{children}</button>
}
function TabPanel({ id, children }: { id: string; children: ReactNode }) {
  return useTabs().active === id ? <div>{children}</div> : null
}
Tabs.Tab = Tab
Tabs.TabPanel = TabPanel
// <Tabs defaultTab="a"><Tabs.Tab id="a">A</Tabs.Tab><Tabs.TabPanel id="a">…</Tabs.TabPanel></Tabs>
```

## Form Handling

### Controlled Forms with Validation

Controlled inputs (`value` + `onChange`), validate on submit, and wire errors accessibly: `<label htmlFor>`, `aria-invalid`, `aria-describedby` pointing at a `role="alert"` message. Disable the submit while pending. Extract this into `useForm` (below) once you have more than one form.

```typescript
export function LoginForm() {
  const login = useSetAtom(loginAtom)
  const navigate = useNavigate()
  const [email, setEmail] = useState('')
  const [errors, setErrors] = useState<{ email?: string }>({})
  const [pending, setPending] = useState(false)

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) return setErrors({ email: 'Invalid email' })
    setPending(true)
    try { await login({ email }); navigate('/dashboard') }
    catch { setErrors({ email: 'Invalid credentials' }) }
    finally { setPending(false) }
  }

  return (
    <form onSubmit={handleSubmit}>
      <label htmlFor="email">Email</label>
      <input id="email" type="email" value={email} onChange={(e) => setEmail(e.target.value)}
        aria-invalid={!!errors.email} aria-describedby={errors.email ? 'email-error' : undefined} />
      {errors.email && <span id="email-error" role="alert">{errors.email}</span>}
      <button type="submit" disabled={pending}>{pending ? 'Logging in…' : 'Log In'}</button>
    </form>
  )
}
```

### Form with Custom Hook

```typescript
// src/hooks/useForm.ts
import { useState, ChangeEvent, FormEvent } from 'react'

interface UseFormOptions<T> {
  initialValues: T
  validate?: (values: T) => Partial<Record<keyof T, string>>
  onSubmit: (values: T) => void | Promise<void>
}

export function useForm<T extends Record<string, any>>({
  initialValues,
  validate,
  onSubmit,
}: UseFormOptions<T>) {
  const [values, setValues] = useState<T>(initialValues)
  const [errors, setErrors] = useState<Partial<Record<keyof T, string>>>({})
  const [isSubmitting, setIsSubmitting] = useState(false)

  const handleChange = (e: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
    const { name, value } = e.target
    setValues((prev) => ({ ...prev, [name]: value }))

    // Clear error for this field
    if (errors[name as keyof T]) {
      setErrors((prev) => {
        const newErrors = { ...prev }
        delete newErrors[name as keyof T]
        return newErrors
      })
    }
  }

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()

    if (validate) {
      const validationErrors = validate(values)
      if (Object.keys(validationErrors).length > 0) {
        setErrors(validationErrors)
        return
      }
    }

    setIsSubmitting(true)
    try {
      await onSubmit(values)
    } finally {
      setIsSubmitting(false)
    }
  }

  const reset = () => {
    setValues(initialValues)
    setErrors({})
    setIsSubmitting(false)
  }

  return {
    values,
    errors,
    isSubmitting,
    handleChange,
    handleSubmit,
    reset,
    setValues,
    setErrors,
  }
}

// Usage: const { values, errors, isSubmitting, handleChange, handleSubmit } =
//   useForm({ initialValues, validate, onSubmit })
// Inputs use name={key} value={values[key]} onChange={handleChange}.
```

For anything beyond trivial forms, consider React 19 Actions (`useActionState`, `<form action={fn}>`) or a schema validator (Zod) instead of hand-rolled validators.

## Best Practices

### Component Organization

```text
src/
├── components/          # Reusable UI components
│   ├── Button/
│   │   ├── Button.tsx
│   │   ├── Button.test.tsx
│   │   └── Button.module.css
│   └── Input/
├── pages/              # Route components
│   ├── HomePage.tsx
│   └── users/
│       ├── UsersPage.tsx
│       └── UserDetailPage.tsx
├── layouts/            # Layout components
│   └── RootLayout.tsx
├── hooks/              # Custom hooks
│   ├── useDebounce.ts
│   └── useForm.ts
├── store/              # Jotai atoms
│   ├── auth.ts
│   ├── cart.ts
│   └── users.ts
├── utils/              # Utility functions
│   └── api.ts
├── types/              # TypeScript types
│   └── index.ts
└── main.tsx           # Entry point
```

### Performance Optimization

Memoization (`memo`/`useMemo`/`useCallback`) is a PERF tool, not a correctness tool, and the React Compiler now automates it — see **Expert Practices** for the mechanism and traps. In new code, prefer architectural fixes (move state down, split components, stable keys) over scattering memoization.

Route-level **code splitting** is the highest-leverage manual win — always split routes with `lazy` + `Suspense`:

```typescript
const DashboardPage = lazy(() => import('./pages/DashboardPage'))
// <Suspense fallback={<Spinner />}><DashboardPage /></Suspense>
```

### Error Boundaries

```typescript
// src/components/ErrorBoundary.tsx
import { Component, ReactNode } from 'react'

interface Props {
  children: ReactNode
  fallback?: (error: Error, reset: () => void) => ReactNode
}

interface State {
  error: Error | null
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props)
    this.state = { error: null }
  }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('Error caught by boundary:', error, errorInfo)
  }

  reset = () => {
    this.setState({ error: null })
  }

  render() {
    if (this.state.error) {
      if (this.props.fallback) {
        return this.props.fallback(this.state.error, this.reset)
      }

      return (
        <div role="alert">
          <h2>Something went wrong</h2>
          <pre>{this.state.error.message}</pre>
          <button onClick={this.reset}>Try again</button>
        </div>
      )
    }

    return this.props.children
  }
}

// Usage
function App() {
  return (
    <ErrorBoundary
      fallback={(error, reset) => (
        <div>
          <h1>Error: {error.message}</h1>
          <button onClick={reset}>Retry</button>
        </div>
      )}
    >
      <YourApp />
    </ErrorBoundary>
  )
}
```

### Accessibility (a11y)

Core rules: prefer semantic elements (`<button>`, `<nav>`, `<main>`) over `<div role>`; every input needs an associated `<label htmlFor>` (or `useId`); errors go in `role="alert"` linked via `aria-describedby`; interactive custom widgets need full keyboard support + `aria-expanded`/`aria-haspopup`/`aria-controls`; announce async results via a live region.

#### Modal Dialog with Focus Management

`role="dialog"` + `aria-modal="true"` + `aria-labelledby`. On open: save `document.activeElement`, focus the dialog, trap Tab, close on Escape, lock body scroll; on cleanup restore focus. Render via `createPortal` to `document.body`.

```typescript
export function Modal({ isOpen, onClose, title, children }: ModalProps) {
  const dialogRef = useRef<HTMLDivElement>(null)
  const prevFocus = useRef<HTMLElement | null>(null)

  useEffect(() => {
    if (!isOpen) return
    prevFocus.current = document.activeElement as HTMLElement
    dialogRef.current?.focus()
    const nodes = dialogRef.current?.querySelectorAll<HTMLElement>(
      'button,[href],input,select,textarea,[tabindex]:not([tabindex="-1"])')
    const first = nodes?.[0], last = nodes?.[nodes.length - 1]

    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') return onClose()
      if (e.key !== 'Tab') return
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus() }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus() }
    }
    document.addEventListener('keydown', onKey)
    document.body.style.overflow = 'hidden'
    return () => {
      document.removeEventListener('keydown', onKey)
      document.body.style.overflow = ''
      prevFocus.current?.focus() // restore focus — critical for keyboard users
    }
  }, [isOpen, onClose])

  if (!isOpen) return null
  return createPortal(
    <div className="modal-overlay" onClick={onClose} role="presentation">
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="modal-title"
        tabIndex={-1} onClick={(e) => e.stopPropagation()}>
        <h2 id="modal-title">{title}</h2>
        {children}
      </div>
    </div>, document.body)
}
```

⚠️ The native `<dialog>` element with `showModal()` gives focus trap + Escape + scroll lock for free — prefer it over a hand-rolled trap unless you need custom overlay behavior.

#### Keyboard Widgets (dropdown/menu essentials)

Trigger: `aria-haspopup`, `aria-expanded={isOpen}`, `aria-controls={menuId}`; open on Enter/Space/ArrowDown. Menu: `role="menu"`, items `role="menuitem"`, focus the first item on open. Keys: Escape closes and returns focus to the trigger; ArrowUp/Down move between items. Same skeleton (roving focus + `aria-*`) applies to comboboxes, listboxes, tabs.

#### Skip Link + Visually-Hidden

Skip link: first focusable element, off-screen until focused, targets `<main id="main-content" tabIndex={-1}>`. Reuse the `.visually-hidden` class for screen-reader-only text and live regions (do NOT use `display:none` — that hides from AT too).

```css
.visually-hidden { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px;
  overflow: hidden; clip: rect(0,0,0,0); white-space: nowrap; border-width: 0; }
.skip-link { position: absolute; left: -10000px; }
.skip-link:focus { left: 0; width: auto; height: auto; } /* CSS :focus, not JS onFocus */
```

#### Live Region for Announcements

A single app-level `aria-live` region announces async results (saves, errors, route changes). Trap: setting the same text twice is NOT re-announced — clear to `''` then set on the next tick to force it. Use `polite` for status, `assertive` for errors.

```typescript
<div role="status" aria-live={priority} aria-atomic="true" className="visually-hidden">{message}</div>
// announce(): setMessage(''); setTimeout(() => setMessage(text), 100)
```

## Anti-Patterns

### Forbidden in this stack

Next.js / Remix (SSR — out of scope), `next/*` imports · create-react-app (deprecated) · webpack configs (use Vite) · Redux/RTK (use Jotai; exception: existing Redux codebases) · Context for hot global state (use Jotai; Context is for ambient subtree values like theme) · class components · default exports (prefer named).

### Common Mistakes

- **Mutation:** `items.push(x); setItems(items)` — React compares by reference, no re-render. Use `setItems([...items, x])` / `setItems(prev => [...prev, x])`.
- **Derived state in Effect:** `useEffect(() => setFiltered(items.filter(f)), [items,f])` — compute during render instead: `const filtered = items.filter(f)`.
- **Missing deps:** every value read inside an Effect belongs in its dep array (enable `eslint-plugin-react-hooks`); don't disable the lint — fix the design.
- **Prop drilling shared state:** lift to a Jotai atom, read via `useAtom` at the leaf.
- **Fetch-in-effect for render data:** prefer a route loader or async atom + `<Suspense>` over `useState`/`useEffect` fetch triads. If you must fetch in an Effect, use the ignore-flag pattern (see Gotchas) to avoid races and setState-after-unmount.

### Quick Pattern Swaps

```typescript
// BAD: Calling Hooks conditionally
function SearchPanel({ enabled }: { enabled: boolean }) {
  if (enabled) {
    useEffect(() => {
      subscribe()
    }, [])
  }
  return null
}

// GOOD: Call Hooks at the top level and branch inside
function SearchPanel({ enabled }: { enabled: boolean }) {
  useEffect(() => {
    if (!enabled) return
    const unsubscribe = subscribe()
    return unsubscribe
  }, [enabled])

  return null
}

// BAD: Using unstable keys
items.map((item, index) => <Row key={index} item={item} />)
items.map((item) => <Row key={Math.random()} item={item} />)

// GOOD: Use stable IDs from the data
items.map((item) => <Row key={item.id} item={item} />)

// BAD: Resetting state in an effect when identity changes
function Messenger({ thread }: { thread: Thread }) {
  const [draft, setDraft] = useState("")

  useEffect(() => {
    setDraft("")
  }, [thread.id])

  return <Composer draft={draft} onDraftChange={setDraft} />
}

// GOOD: Reset the stateful subtree with a key
function Messenger({ thread }: { thread: Thread }) {
  return <Composer key={thread.id} thread={thread} />
}
```

### Core Hook Pattern Swaps

Defaults for refs vs state vs effects vs memo vs shared state. Most rationale is in **Expert Practices**; quick rules:

- UI value that affects render → `useState`, not a ref. Refs hold non-render values (DOM nodes, timers, latest-value stashes).
- Value derivable from props/state → compute during render, never `useEffect` + `setState`.
- `useCallback`/`useMemo` only to stabilize props for a memoized child/Effect or for genuinely expensive compute — not by default (the Compiler handles the rest).
- Related fields with coupled transitions → `useReducer`; hot global state → Jotai; ambient subtree value → Context.

**Latest value inside an Effect callback** (a subscription handler that must see fresh `theme` without re-subscribing on every `theme` change) → `useEffectEvent`, not a manually-synced ref:

```typescript
// GOOD: Use useEffectEvent when Effect-driven callbacks need the latest values
// useEffectEvent is stable as of React 19.2 (Oct 2025); on 19.0/19.1 importing
// it from 'react' fails — use a ref to hold the latest value instead. It may
// ONLY be called from inside Effects or other Effect Events — never call it
// during render, never pass it to a child component or Hook, and never list it
// in a dependency array (its identity changes every render by design). Upgrade
// eslint-plugin-react-hooks to @latest so the linter treats it as a non-dependency.
function ChatRoom({ roomId, theme }: { roomId: string; theme: Theme }) {
  const onConnected = useEffectEvent(() => {
    showToast("Connected", theme)
  })

  useEffect(() => {
    const connection = createConnection(roomId)
    connection.on("connected", onConnected)
    connection.connect()

    return () => connection.disconnect()
  }, [roomId])
}

// useReducer: consolidate related fields with explicit transitions rather than
// many useState + cross-field sync. Reducer is a pure (state, action) => state.
type FormState = { status: 'idle' | 'saving' | 'error'; data: { name: string }; error: string | null }
type FormAction =
  | { type: 'changed_name'; value: string }
  | { type: 'save_started' } | { type: 'save_failed'; message: string } | { type: 'save_succeeded' }

function formReducer(state: FormState, action: FormAction): FormState {
  switch (action.type) {
    case 'changed_name':   return { ...state, data: { ...state.data, name: action.value } }
    case 'save_started':   return { ...state, status: 'saving', error: null }
    case 'save_failed':    return { ...state, status: 'error', error: action.message }
    case 'save_succeeded': return { ...state, status: 'idle', error: null }
  }
}
// const [state, dispatch] = useReducer(formReducer, { status: 'idle', data: { name: '' }, error: null })

// Animation: requestAnimationFrame (not setInterval), and bail on reduced-motion.
useEffect(() => {
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
  let frame = requestAnimationFrame(function tick(t) { drawFrame(t); frame = requestAnimationFrame(tick) })
  return () => cancelAnimationFrame(frame)
}, [])
```

## Testing

Vitest + React Testing Library (jsdom). Config: `test: { globals: true, environment: 'jsdom', setupFiles: './src/test/setup.ts' }` in `vite.config.ts`; in `setup.ts` extend `expect` with `@testing-library/jest-dom/matchers` and `afterEach(cleanup)`.

Test behavior via accessible roles/text, not implementation. Query with `getByRole`/`getByLabelText` (which also enforce a11y); use `userEvent` over `fireEvent` for realistic interaction; wrap state updates in `act`.

```typescript
it('is disabled while loading', () => {
  render(<Button isLoading>Save</Button>)
  expect(screen.getByRole('button', { name: /save/i })).toBeDisabled()
})

// Atoms are plain — test via renderHook; act() around writes.
it('increments', () => {
  const { result } = renderHook(() => useAtom(countAtom))
  act(() => result.current[1]((c) => c + 1))
  expect(result.current[0]).toBe(1)
})
```

⚠️ Each Jotai test needs isolation: atoms hold module-level identity but their VALUES live in a `Provider`'s store. Wrap `renderHook`/`render` in a fresh `<Provider>` (or `createStore()`) per test so state does not leak between tests.

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal rules an expert applies reflexively, with the mechanism that makes each one true. Group: **Idioms** (the current way), **Anti-Patterns** (what corrupts React's machinery), **Gotchas** (correct-looking code that fails), **Performance**, **Currency** (React 19.2 / Compiler).

### Idioms

**Render `<Context>` directly as the provider — `.Provider` is no longer needed.** React 19 lets the context object itself be the provider: `<ThemeContext value={...}>`. Behavior is identical; it is pure syntactic simplification. `.Provider` is NOT yet formally deprecated in 19.x (the blog says "in future versions we will deprecate `<Context.Provider>`"), but the direct form is forward-looking and a codemod exists. Use one form consistently — mixing both in a codebase is confusing.

```typescript
const ThemeContext = createContext<Theme>('light')

function App() {
  return (
    <ThemeContext value="dark">
      <Page />
    </ThemeContext>
  )
}
```

**Pass `ref` as a regular prop — `forwardRef` is the legacy pattern.** React 19 stopped stripping `ref` out of the props bag, so the `forwardRef` HOC indirection is obsolete and you can drop `.displayName` (a named function declares itself). Precision: in 19.0–19.2 `forwardRef` still works WITHOUT a deprecation warning — the blog only says it "will deprecate and remove forwardRef" in a future version. So writing new `forwardRef` wrappers is legacy (and will break later) but does not currently warn. The `react-19` codemod migrates existing usages.

**Subscribe to external stores with `useSyncExternalStore`, never `useEffect` + `useState`.** The effect approach has a window between subscription start and the first snapshot read where the store can change, producing a *tear* (components in one render see different versions); cleanup is also manual. `useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot)` reads the snapshot synchronously during render and resubscribes if `subscribe` changes. **Pitfall:** `getSnapshot` must be referentially stable — returning a fresh object literal each call triggers an infinite re-render loop. Return cached or primitive values.

```typescript
const subscribe = (cb: () => void) => {
  window.addEventListener('online', cb)
  window.addEventListener('offline', cb)
  return () => {
    window.removeEventListener('online', cb)
    window.removeEventListener('offline', cb)
  }
}
const getSnapshot = () => navigator.onLine

export function useOnlineStatus() {
  return useSyncExternalStore(subscribe, getSnapshot, () => true)
}
```

**`useActionState`'s action receives `(previousState, formData)` — state FIRST.** Imported from `'react'` (not `'react-dom'`; `useFormState` is deprecated), it returns `[state, formAction, isPending]` and guarantees ordering, so rapid resubmits resolve to the last completed action. Two real traps: (1) writing `async (formData) => formData.get('name')` silently calls `.get` on the *previous state* and returns null for every field — state is the first arg. (2) Error convention: **RETURN** expected/validation errors (they become the new state); **THROW** unexpected errors (they reach the nearest Error Boundary). The new state is whatever the action returns — it is NOT auto-reset to the initial value on success.

```typescript
import { useActionState } from 'react' // NOT 'react-dom'

async function updateProfile(prev: { error?: string; message: string }, formData: FormData) {
  const name = formData.get('name') as string // correct: formData is the 2nd arg
  if (!name) return { error: 'Name is required', message: '' } // returned validation error
  await saveProfile(name)
  return { message: 'Saved' }
}

const [state, formAction, isPending] = useActionState(updateProfile, { message: '' })
```

### Anti-Patterns

**Never create the Promise inside the component when using `use()`.** "Promises created in Client Components are recreated on every render." Calling `fetchUser(userId)` inline and passing it to `use()` mints a new Promise identity each render — each new identity re-suspends, re-renders, and re-fetches in a loop. Create the Promise outside the render cycle: a React Router loader (stable per navigation, the SPA idiom here), a Jotai async atom (caches per key), or a `useState` lazy initializer. `use()` cannot be wrapped in try/catch — handle rejection with an Error Boundary.

**Never call a component as a plain function — invoke only via JSX.** `Component()` instead of `<Component />` bypasses React's fiber machinery. React keeps an ordered hook registry per fiber; a direct call binds the called component's hooks to the *caller's* fiber, so they appear to work but corrupt on the next conditional render. Direct calls also break context lookups, StrictMode double-invocation, DevTools, and error boundaries — all of which require React to control invocation. The Rules of React forbid it.

**Memoization silently fails when ANY prop is an unstable reference — including `children`.** `React.memo` skips re-render only when ALL props are shallowly equal. A single inline `style={{...}}`, inline array, or inline arrow crosses the boundary every render, paying the comparison cost with no benefit. `children` is especially treacherous: `<Memo><p>Hi</p></Memo>` always busts memo because `<p>Hi</p>` is a new element object each render. The optimization is also invisible to callers — a later `items ?? []` silently breaks it. `memo` is reliable mainly for components with no props or only primitive props. Prefer architectural fixes (move state down, composition, split components) over pervasive memoization.

```typescript
// BAD: inline style busts memo every render
<MemoizedList items={items} style={{ margin: 0 }} />

// GOOD: move state to where it is used; keep memoized children in a hoisted element
const stableChildren = <p>Static content</p>
<MemoizedBox>{stableChildren}</MemoizedBox>
```

### Gotchas

**State lives at position-in-tree + type — never define components inside render; use `key` to reset.** React's reconciler identifies a component by its tree position combined with its type (the function reference). (1) Same type at the same position preserves state across different props — `isFancy ? <Counter isFancy /> : <Counter />` keeps the counter's state, leaking it to the wrong context; force a fresh instance with `key`. (2) Defining a child component INSIDE a parent's render creates a new function reference every render, so React sees a different type and unmounts/remounts the child — the classic "input resets while typing" bug. Always declare components at module top level. `key` is also the correct way to reset subtree state on identity change — far better than `useEffect(() => setState(initial), [id])`, which double-renders with a visible stale frame.

**Multiple `setState` calls in one handler read ONE snapshot — use the updater form.** A state variable is a fixed snapshot for the whole handler, so `setCount(count + 1)` three times increments by 1, not 3. React 18+ batches async callbacks, timeouts, and Promises too, widening the trap. Use `setCount(c => c + 1)` when an update depends on the prior value — updaters are queued and each receives the previous result. Especially important after `await`, where a value captured before the await is stale.

```typescript
setCount(c => c + 1)
setCount(c => c + 1)
setCount(c => c + 1) // increments by 3
```

**Async Effects race on fast mount/unmount — guard with an ignore flag (or `AbortController`).** When an Effect fetches, a rapid unmount→remount (or changing dep) fires a second fetch before the first resolves; responses can arrive out of order, so the STALE one wins and setState fires after unmount. Cleanup cannot cancel an in-flight `await`, so gate the state write on a per-effect flag. StrictMode's dev double-mount deliberately surfaces this. (In this stack, a route loader or async atom sidesteps it entirely — prefer those for render data.)

```typescript
useEffect(() => {
  let ignore = false
  fetchUser(userId).then((u) => { if (!ignore) setUser(u) }) // ignore the stale response
  return () => { ignore = true }
}, [userId])
```

**`useRef`: do not read or write `ref.current` during render (one exception: lazy init).** Concurrent React may render a component multiple times before committing; ref mutations persist across those phantom renders while state does not, so render-phase writes violate purity and produce inconsistent results. Reads/writes belong in effects and event handlers. The one documented exception is idempotent lazy init: read `ref.current`, and if null, set it. Related waste: `useRef(new Expensive())` runs the constructor on EVERY render (the result is discarded after the first) — use the null-guard pattern.

```typescript
const playerRef = useRef<VideoPlayer | null>(null)
if (playerRef.current === null) {
  playerRef.current = new VideoPlayer() // idempotent lazy init — the only render-phase exception
}
```

**React 19 TypeScript: ref callbacks with implicit returns are now rejected — use a block body.** React 19 added ref cleanup functions (a ref callback may `return () => cleanup()`, called on unmount). As a side effect the types reject ANY non-function return, because `(node) => (instance = node)` is ambiguous against an intentional cleanup. This compiles in JS but is a TS build-breaking change; the fix is mechanical (block-body arrow). The `react-19` codemod includes `no-implicit-ref-callback-return`.

```typescript
<div ref={(node) => { instance = node }} /> // explicit block body — required

<input ref={(node) => {        // ref-with-cleanup: the reason the change exists
  if (!node) return
  subscribe(node)
  return () => unsubscribe(node)
}} />
```

**`useOptimistic` reverts SILENTLY on error — add explicit feedback and group related state.** When the async Action inside `startTransition` rejects, React reverts the optimistic value but shows NO error UI — the interaction looks successful, then snaps back. Always catch and set explicit error state. The optimistic setter must run inside a Transition/Action context (from a plain handler it warns and reverts immediately). Second trap: separate `useOptimistic` calls for related values revert independently, briefly showing an inconsistent UI — group related optimistic state into one `useOptimistic` with a reducer so it reverts atomically.

```typescript
startTransition(async () => {
  addOptimisticItem(newItem)
  try {
    await saveItem(newItem)
  } catch (err) {
    setError((err as Error).message) // explicit feedback — NOT automatic
  }
})

const [optimistic, dispatch] = useOptimistic(
  { isFollowing: false, count: 0 },
  (state, follow: boolean) => ({ isFollowing: follow, count: state.count + (follow ? 1 : -1) })
)
```

**`StrictMode` double-invokes renders, initializers, and effects to surface impurity and missing cleanup.** In development it runs component bodies twice, calls `useState`/`useReducer` initializers twice, and runs each Effect through setup→cleanup→setup. This is intentional and exposes real bugs: impure renders, side-effecting initializers, and Effects missing cleanup (which leak). Production runs effects once, so a missing cleanup "works" until a fast mount/unmount (rapid navigation) triggers the leak — StrictMode forces that cycle in dev. Make every Effect survive setup→cleanup→setup (always return cleanup) and keep initializers idempotent. You cannot opt a subtree out.

**React Router `errorElement`/`ErrorBoundary` catches loader/action/render errors — NOT event-handler or effect errors.** A manual fetch in an `onClick` or inside `useEffect` fails silently unless you try/catch it yourself. Second trap: check `isRouteErrorResponse(error)` FIRST to distinguish intentional HTTP errors (`throw data(..., { status: 404 })` from a loader) from real JS `Error`s — otherwise a deliberate 404 renders as a crash with `error.message` undefined. Throw in loaders for 404/401; RETURN form-validation/control-flow results, do not throw them.

```typescript
export function ErrorBoundary() {
  const error = useRouteError()
  if (isRouteErrorResponse(error)) return <NotFoundPage status={error.status} /> // intentional HTTP error
  if (error instanceof Error) return <CrashPage message={error.message} />        // unexpected JS error
  return <p>Unknown error</p>
}
```

**Jotai `atomWithStorage` causes a hydration mismatch under SSR/SSG only.** It reads localStorage on the client but has no storage on the server, so the first client render diverges from server HTML, tripping React's hydration warning and a visible flash. This is purely an SSR/SSG/pre-rendering concern — in a pure client-side SPA (this skill's target) there is no server HTML to mismatch, so it does not apply. If SSR is ever introduced, wrap storage-dependent UI in a client-only boundary (render after mount) or use `useHydrateAtoms`. (Avoid relying on a specific `delayInit`-style option — Jotai's storage API has changed across versions; verify current options first.)

### Performance & Currency (React 19.2 / Compiler)

**Wrap navigations in `startTransition` so Suspense does not flash a fallback.** Updating state that re-suspends an already-revealed boundary instantly replaces visible content with the fallback — even if new data is 100ms away. Inside a Transition, React keeps rendering the old tree while preparing the new one and does not replace already-revealed content; `isPending` gives a lightweight indicator (e.g. dimming). **Gotcha:** in React 19 `startTransition` accepts async functions, but because JS lacks AsyncContext, state updates AFTER the first `await` lose the Transition scope and become urgent (can flash) — re-wrap post-await setters in a nested `startTransition`, or use `useActionState` which handles ordering.

```typescript
const [isPending, startTransition] = useTransition()
const navigate = (url: string) => startTransition(() => setPage(url))
// <div style={{ opacity: isPending ? 0.6 : 1 }}> ... <Suspense> ... </Suspense> </div>
```

**Match Suspense boundaries to the UX loading sequence — not one per data-fetching component.** "Suspense boundaries should not be more granular than the loading sequence that you want the user to experience." Per-component spinners pop in out of order and feel chaotic; a single global boundary is usually wrong too. Two fetches that should appear together belong under ONE boundary; when one section is much slower, NEST a boundary to reveal the fast part first. This is a UX decision driven by the design.

```typescript
// Bio is much slower: nest to reveal Avatar first
<Suspense fallback={<BigSpinner />}>
  <Avatar userId={id} />
  <Suspense fallback={<BioSkeleton />}>
    <Bio userId={id} />
  </Suspense>
</Suspense>
```

**React Compiler v1.0 (stable Oct 7, 2025) automates memoization — write plain components in NEW code.** A build-time transform inserts the equivalent of `useMemo`/`useCallback`/`React.memo` via dataflow analysis, and can memoize conditionally and past early returns (which manual memoization cannot). With the compiler enabled, do not pre-emptively scatter memoization in new code. Precision: the React team does NOT say to strip existing manual memoization — `useMemo`/`useCallback` "can continue to be used with React Compiler as an escape hatch" for precise control, and for existing code they recommend leaving it in place or testing carefully before removing, since removal can change compiler output. Supports React 17+ (add `react-compiler-runtime` for pre-19).

**`<Activity>` (React 19.2) hides UI without unmounting — replaces `display:none` and conditional-render hacks.** With `mode="hidden"` it "hides the children, unmounts effects, and defers all updates until React has nothing left to work on" while preserving state and DOM, so revealing it is instant with no re-mount. This beats conditional rendering (`{cond && <X/>}`), which destroys state, and a CSS `display:none` wrapper, which preserves state but does NOT pause effects/free resources. Use it for tab panels that should keep scroll/input state and for pre-rendering routes during navigation.

```typescript
import { Activity } from 'react' // React 19.2+

function Tabs({ activeTab }: { activeTab: string }) {
  return (
    <>
      <Activity mode={activeTab === 'profile' ? 'visible' : 'hidden'}>
        <ProfilePanel />
      </Activity>
      <Activity mode={activeTab === 'settings' ? 'visible' : 'hidden'}>
        <SettingsPanel />
      </Activity>
    </>
  )
}
```

## Verify Before Done

- [ ] No `useEffect`+`setState` that only derives a value from props/state (compute in render)
- [ ] Effects that touch external systems return cleanup; async effects use an ignore-flag/AbortController; survives StrictMode setup→cleanup→setup
- [ ] Updates depending on prior state use the updater form (`setX(x => …)`), especially after `await`
- [ ] Lists use stable data IDs for `key`; subtree resets use `key={id}`, not an Effect
- [ ] Every Jotai atom is defined at module scope (per-id → `atomFamily`); `use()` promises come from a loader/atom, never created in render
- [ ] `memo`/`useMemo`/`useCallback` used only for perf (stable props for memoized children or expensive compute), not to fix bugs — no unstable inline props defeating a `memo`
- [ ] No `forwardRef` in new code (ref is a plain prop); ref callbacks use block bodies
- [ ] Interactive elements are keyboard-operable with correct roles/`aria-*`; inputs have labels; errors in `role="alert"` via `aria-describedby`
- [ ] Not using Next.js/Remix/Redux/CRA/webpack/class components
- [ ] `tsc` and lint clean (`eslint-plugin-react-hooks` enabled, deps not suppressed)
