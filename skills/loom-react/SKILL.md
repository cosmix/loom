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

This skill provides guidance for building modern React Single Page Applications (SPAs) using:

- **React 19+** for UI components and hooks
- **React Router v7** for client-side routing and navigation
- **Jotai** for atomic global state management
- **Vite** for fast development and optimized builds
- **Bun** as the package manager and runtime

This skill focuses exclusively on client-side React applications, NOT server-side rendering frameworks like Next.js or Remix.

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

### Document Metadata Components

```typescript
import { useEffect } from 'react'

// React 19 allows rendering metadata in components
export function ProductPage({ product }: { product: Product }) {
  return (
    <div>
      <title>{product.name} - My Store</title>
      <meta name="description" content={product.description} />
      <meta property="og:title" content={product.name} />

      <h1>{product.name}</h1>
      <p>{product.description}</p>
    </div>
  )
}
```

## Server Components vs Client Components

**Important**: This skill focuses on CLIENT-SIDE SPAs. However, when working with frameworks that support React Server Components (RSC):

### When NOT Using Next.js/Remix (This Skill's Focus)

All components are client components by default in SPAs. You have full access to:

- Browser APIs (window, document, localStorage)
- Event handlers (onClick, onChange, etc.)
- React hooks (useState, useEffect, etc.)
- Client-side routing

### When Using Next.js or Remix (Outside This Skill)

Server Components are the default and cannot use:

- Client-side hooks or state
- Browser APIs
- Event handlers

Mark components with `'use client'` to make them client components:

```typescript
'use client'

import { useState } from 'react'

export function Counter() {
  const [count, setCount] = useState(0)
  return <button onClick={() => setCount(count + 1)}>{count}</button>
}
```

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

### Card Component with Composition

```typescript
// src/components/ui/Card.tsx
// React 19: ref is a plain prop, so each primitive is a plain function component.
import { ComponentPropsWithoutRef } from 'react'

type DivProps = ComponentPropsWithoutRef<'div'> & { ref?: React.Ref<HTMLDivElement> }
type H3Props = ComponentPropsWithoutRef<'h3'> & { ref?: React.Ref<HTMLHeadingElement> }

export function Card({ ref, className, ...props }: DivProps) {
  return (
    <div
      ref={ref}
      className={`rounded-lg border bg-white shadow-sm ${className}`}
      {...props}
    />
  )
}

export function CardHeader({ ref, className, ...props }: DivProps) {
  return <div ref={ref} className={`p-6 ${className}`} {...props} />
}

export function CardTitle({ ref, className, ...props }: H3Props) {
  return <h3 ref={ref} className={`text-2xl font-semibold ${className}`} {...props} />
}

export function CardContent({ ref, className, ...props }: DivProps) {
  return <div ref={ref} className={`p-6 pt-0 ${className}`} {...props} />
}

// Usage
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/Card'

export function ProductCard({ product }: { product: Product }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{product.name}</CardTitle>
      </CardHeader>
      <CardContent>
        <p>{product.description}</p>
        <p className="text-xl font-bold">${product.price}</p>
      </CardContent>
    </Card>
  )
}
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

### Render Props Pattern

```typescript
// src/components/DataLoader.tsx
import { ReactNode } from 'react'

interface DataLoaderProps<T> {
  data: T | null
  isLoading: boolean
  error: Error | null
  children: (data: T) => ReactNode
  loadingFallback?: ReactNode
  errorFallback?: (error: Error) => ReactNode
}

export function DataLoader<T>({
  data,
  isLoading,
  error,
  children,
  loadingFallback = <div>Loading...</div>,
  errorFallback = (err) => <div>Error: {err.message}</div>,
}: DataLoaderProps<T>) {
  if (isLoading) return <>{loadingFallback}</>
  if (error) return <>{errorFallback(error)}</>
  if (!data) return null

  return <>{children(data)}</>
}

// Usage
import { useFetch } from '@/hooks/useFetch'

export function UsersList() {
  const { data, isLoading, error } = useFetch<User[]>('/api/users')

  return (
    <DataLoader data={data} isLoading={isLoading} error={error}>
      {(users) => (
        <ul>
          {users.map((user) => (
            <li key={user.id}>{user.name}</li>
          ))}
        </ul>
      )}
    </DataLoader>
  )
}
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

```typescript
// src/layouts/RootLayout.tsx
import { Outlet, Link, useNavigation } from 'react-router'

export function RootLayout() {
  const navigation = useNavigation()
  const isNavigating = navigation.state === 'loading'

  return (
    <div className="app">
      <header>
        <nav>
          <Link to="/">Home</Link>
          <Link to="/about">About</Link>
          <Link to="/users">Users</Link>
        </nav>
      </header>

      <main>
        {isNavigating && <div className="loading-bar">Loading...</div>}
        <Outlet />
      </main>

      <footer>
        <p>&copy; 2025 My App</p>
      </footer>
    </div>
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

```typescript
// src/components/UserForm.tsx
import { useNavigate, useSearchParams } from 'react-router'
import { useState } from 'react'

export function UserForm() {
  const navigate = useNavigate()
  const [searchParams, setSearchParams] = useSearchParams()
  const [name, setName] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()

    // Create user
    const response = await fetch('/api/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name }),
    })

    const user = await response.json()

    // Navigate programmatically
    navigate(`/users/${user.id}`)
  }

  const filter = searchParams.get('filter') || ''

  return (
    <form onSubmit={handleSubmit}>
      <input
        type="text"
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="User name"
      />

      <input
        type="text"
        value={filter}
        onChange={(e) => setSearchParams({ filter: e.target.value })}
        placeholder="Filter"
      />

      <button type="submit">Create User</button>
    </form>
  )
}
```

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

```typescript
// src/store/users.ts
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

interface User {
  id: string
  name: string
  email: string
}

// Async atom for fetching users
export const usersAtom = atom(async () => {
  const response = await fetch('/api/users')
  if (!response.ok) {
    throw new Error('Failed to fetch users')
  }
  return response.json() as Promise<User[]>
})

// Atom with refresh capability
export const refreshUsersAtom = atom(0)

export const refreshableUsersAtom = atom(async (get) => {
  get(refreshUsersAtom) // Dependency for refreshing
  const response = await fetch('/api/users')
  return response.json() as Promise<User[]>
})

// Usage in component
import { useAtomValue, useSetAtom } from 'jotai'
import { Suspense } from 'react'

function UsersList() {
  const users = useAtomValue(usersAtom)

  return (
    <ul>
      {users.map((user) => (
        <li key={user.id}>{user.name}</li>
      ))}
    </ul>
  )
}

export function UsersContainer() {
  return (
    <Suspense fallback={<div>Loading users...</div>}>
      <UsersList />
    </Suspense>
  )
}
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

### Complex State with Atom Composition

```typescript
// src/store/cart.ts
import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";

interface CartItem {
  productId: string;
  quantity: number;
  price: number;
}

// Persisted cart items
export const cartItemsAtom = atomWithStorage<CartItem[]>("cart", []);

// Derived: total items count
export const cartCountAtom = atom((get) => {
  const items = get(cartItemsAtom);
  return items.reduce((sum, item) => sum + item.quantity, 0);
});

// Derived: total price
export const cartTotalAtom = atom((get) => {
  const items = get(cartItemsAtom);
  return items.reduce((sum, item) => sum + item.price * item.quantity, 0);
});

// Action: add to cart
export const addToCartAtom = atom(
  null,
  (get, set, item: Omit<CartItem, "quantity"> & { quantity?: number }) => {
    const items = get(cartItemsAtom);
    const existingIndex = items.findIndex(
      (i) => i.productId === item.productId,
    );

    if (existingIndex !== -1) {
      const newItems = [...items];
      const existing = newItems[existingIndex]!;
      newItems[existingIndex] = {
        ...existing,
        quantity: existing.quantity + (item.quantity ?? 1),
      };
      set(cartItemsAtom, newItems);
    } else {
      set(cartItemsAtom, [...items, { ...item, quantity: item.quantity ?? 1 }]);
    }
  },
);

// Action: remove from cart
export const removeFromCartAtom = atom(null, (get, set, productId: string) => {
  const items = get(cartItemsAtom);
  set(
    cartItemsAtom,
    items.filter((item) => item.productId !== productId),
  );
});

// Action: clear cart
export const clearCartAtom = atom(null, (get, set) => {
  set(cartItemsAtom, []);
});
```

## Component Patterns

### Functional Components with TypeScript

```typescript
// src/components/Button.tsx
import { ComponentPropsWithoutRef } from 'react'

interface ButtonProps extends ComponentPropsWithoutRef<'button'> {
  variant?: 'primary' | 'secondary' | 'danger'
  size?: 'sm' | 'md' | 'lg'
  isLoading?: boolean
  ref?: React.Ref<HTMLButtonElement>
}

// React 19: ref arrives as a regular prop — no forwardRef, no displayName.
export function Button({
  ref,
  variant = 'primary',
  size = 'md',
  isLoading,
  children,
  ...props
}: ButtonProps) {
  return (
    <button
      ref={ref}
      className={`btn btn-${variant} btn-${size}`}
      disabled={isLoading || props.disabled}
      {...props}
    >
      {isLoading ? 'Loading...' : children}
    </button>
  )
}
```

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

// Usage
function SearchInput() {
  const [search, setSearch] = useState('')
  const debouncedSearch = useDebounce(search, 300)

  useEffect(() => {
    if (debouncedSearch) {
      // Perform search
      fetchResults(debouncedSearch)
    }
  }, [debouncedSearch])

  return (
    <input
      type="text"
      value={search}
      onChange={(e) => setSearch(e.target.value)}
    />
  )
}
```

```typescript
// src/hooks/useLocalStorage.ts
import { useState, useEffect } from "react";

export function useLocalStorage<T>(
  key: string,
  initialValue: T,
): [T, (value: T | ((val: T) => T)) => void] {
  const [storedValue, setStoredValue] = useState<T>(() => {
    try {
      const item = window.localStorage.getItem(key);
      return item ? JSON.parse(item) : initialValue;
    } catch (error) {
      console.error(error);
      return initialValue;
    }
  });

  const setValue = (value: T | ((val: T) => T)) => {
    try {
      const valueToStore =
        value instanceof Function ? value(storedValue) : value;
      setStoredValue(valueToStore);
      window.localStorage.setItem(key, JSON.stringify(valueToStore));
    } catch (error) {
      console.error(error);
    }
  };

  return [storedValue, setValue];
}
```

```typescript
// src/hooks/useFetch.ts
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

```typescript
// src/components/Tabs/Tabs.tsx
import {
  createContext,
  useContext,
  useState,
  ReactNode,
} from 'react'

interface TabsContextValue {
  activeTab: string
  setActiveTab: (id: string) => void
}

const TabsContext = createContext<TabsContextValue | undefined>(undefined)

function useTabs() {
  const context = useContext(TabsContext)
  if (!context) {
    throw new Error('Tabs components must be used within <Tabs>')
  }
  return context
}

interface TabsProps {
  defaultTab: string
  children: ReactNode
}

export function Tabs({ defaultTab, children }: TabsProps) {
  const [activeTab, setActiveTab] = useState(defaultTab)

  return (
    <TabsContext value={{ activeTab, setActiveTab }}>
      <div className="tabs">{children}</div>
    </TabsContext>
  )
}

interface TabListProps {
  children: ReactNode
}

function TabList({ children }: TabListProps) {
  return <div className="tab-list">{children}</div>
}

interface TabProps {
  id: string
  children: ReactNode
}

function Tab({ id, children }: TabProps) {
  const { activeTab, setActiveTab } = useTabs()

  return (
    <button
      className={`tab ${activeTab === id ? 'active' : ''}`}
      onClick={() => setActiveTab(id)}
    >
      {children}
    </button>
  )
}

interface TabPanelsProps {
  children: ReactNode
}

function TabPanels({ children }: TabPanelsProps) {
  return <div className="tab-panels">{children}</div>
}

interface TabPanelProps {
  id: string
  children: ReactNode
}

function TabPanel({ id, children }: TabPanelProps) {
  const { activeTab } = useTabs()

  if (activeTab !== id) return null

  return <div className="tab-panel">{children}</div>
}

// Export compound components
Tabs.TabList = TabList
Tabs.Tab = Tab
Tabs.TabPanels = TabPanels
Tabs.TabPanel = TabPanel

// Usage
export function Example() {
  return (
    <Tabs defaultTab="profile">
      <Tabs.TabList>
        <Tabs.Tab id="profile">Profile</Tabs.Tab>
        <Tabs.Tab id="settings">Settings</Tabs.Tab>
        <Tabs.Tab id="notifications">Notifications</Tabs.Tab>
      </Tabs.TabList>

      <Tabs.TabPanels>
        <Tabs.TabPanel id="profile">
          <h2>Profile Content</h2>
        </Tabs.TabPanel>
        <Tabs.TabPanel id="settings">
          <h2>Settings Content</h2>
        </Tabs.TabPanel>
        <Tabs.TabPanel id="notifications">
          <h2>Notifications Content</h2>
        </Tabs.TabPanel>
      </Tabs.TabPanels>
    </Tabs>
  )
}
```

## Form Handling

### Controlled Forms with Validation

```typescript
// src/components/LoginForm.tsx
import { FormEvent, useState } from 'react'
import { useNavigate } from 'react-router'
import { useSetAtom } from 'jotai'
import { loginAtom } from '@store/auth'

interface FormErrors {
  email?: string
  password?: string
}

export function LoginForm() {
  const navigate = useNavigate()
  const login = useSetAtom(loginAtom)

  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [errors, setErrors] = useState<FormErrors>({})
  const [isSubmitting, setIsSubmitting] = useState(false)

  const validate = (): boolean => {
    const newErrors: FormErrors = {}

    if (!email) {
      newErrors.email = 'Email is required'
    } else if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
      newErrors.email = 'Invalid email format'
    }

    if (!password) {
      newErrors.password = 'Password is required'
    } else if (password.length < 8) {
      newErrors.password = 'Password must be at least 8 characters'
    }

    setErrors(newErrors)
    return Object.keys(newErrors).length === 0
  }

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()

    if (!validate()) {
      return
    }

    setIsSubmitting(true)

    try {
      await login({ email, password })
      navigate('/dashboard')
    } catch (error) {
      setErrors({
        email: 'Invalid credentials',
      })
    } finally {
      setIsSubmitting(false)
    }
  }

  return (
    <form onSubmit={handleSubmit}>
      <div>
        <label htmlFor="email">Email</label>
        <input
          id="email"
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          aria-invalid={!!errors.email}
          aria-describedby={errors.email ? 'email-error' : undefined}
        />
        {errors.email && (
          <span id="email-error" role="alert">
            {errors.email}
          </span>
        )}
      </div>

      <div>
        <label htmlFor="password">Password</label>
        <input
          id="password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          aria-invalid={!!errors.password}
          aria-describedby={errors.password ? 'password-error' : undefined}
        />
        {errors.password && (
          <span id="password-error" role="alert">
            {errors.password}
          </span>
        )}
      </div>

      <button type="submit" disabled={isSubmitting}>
        {isSubmitting ? 'Logging in...' : 'Log In'}
      </button>
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

// Usage
interface ContactFormData {
  name: string
  email: string
  message: string
}

export function ContactForm() {
  const { values, errors, isSubmitting, handleChange, handleSubmit } =
    useForm<ContactFormData>({
      initialValues: {
        name: '',
        email: '',
        message: '',
      },
      validate: (values) => {
        const errors: Partial<Record<keyof ContactFormData, string>> = {}

        if (!values.name) {
          errors.name = 'Name is required'
        }

        if (!values.email) {
          errors.email = 'Email is required'
        } else if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(values.email)) {
          errors.email = 'Invalid email'
        }

        if (!values.message) {
          errors.message = 'Message is required'
        }

        return errors
      },
      onSubmit: async (values) => {
        await fetch('/api/contact', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(values),
        })
      },
    })

  return (
    <form onSubmit={handleSubmit}>
      <input
        name="name"
        value={values.name}
        onChange={handleChange}
        placeholder="Name"
      />
      {errors.name && <span>{errors.name}</span>}

      <input
        name="email"
        value={values.email}
        onChange={handleChange}
        placeholder="Email"
      />
      {errors.email && <span>{errors.email}</span>}

      <textarea
        name="message"
        value={values.message}
        onChange={handleChange}
        placeholder="Message"
      />
      {errors.message && <span>{errors.message}</span>}

      <button type="submit" disabled={isSubmitting}>
        {isSubmitting ? 'Sending...' : 'Send'}
      </button>
    </form>
  )
}
```

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

```typescript
import { memo, useMemo, useCallback, lazy, Suspense } from 'react'

// Memoize expensive components
export const ExpensiveList = memo(function ExpensiveList({
  items,
}: {
  items: Item[]
}) {
  return (
    <ul>
      {items.map((item) => (
        <li key={item.id}>{item.name}</li>
      ))}
    </ul>
  )
})

// Memoize expensive calculations
function FilteredList({ items, filter }: { items: Item[]; filter: string }) {
  const filteredItems = useMemo(() => {
    return items.filter((item) => item.name.includes(filter))
  }, [items, filter])

  return <ExpensiveList items={filteredItems} />
}

// Memoize callbacks when a memoized child depends on function identity
const Child = memo(function Child({ onClick }: { onClick: () => void }) {
  return <button onClick={onClick}>Increment</button>
})

function Parent() {
  const [count, setCount] = useState(0)

  const handleClick = useCallback(() => {
    setCount((c) => c + 1)
  }, [])

  return <Child onClick={handleClick} />
}

// Code splitting with lazy loading
const DashboardPage = lazy(() => import('./pages/DashboardPage'))

function App() {
  return (
    <Suspense fallback={<div>Loading...</div>}>
      <DashboardPage />
    </Suspense>
  )
}
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

#### Modal Dialog with Focus Management

```typescript
// src/components/Modal.tsx
import { useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'

interface ModalProps {
  isOpen: boolean
  onClose: () => void
  title: string
  children: React.ReactNode
}

export function Modal({ isOpen, onClose, title, children }: ModalProps) {
  const dialogRef = useRef<HTMLDivElement>(null)
  const previousActiveElement = useRef<HTMLElement | null>(null)

  useEffect(() => {
    if (isOpen) {
      previousActiveElement.current = document.activeElement as HTMLElement
      dialogRef.current?.focus()

      // Trap focus inside modal
      const focusableElements = dialogRef.current?.querySelectorAll(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      )
      const firstElement = focusableElements?.[0] as HTMLElement
      const lastElement = focusableElements?.[
        focusableElements.length - 1
      ] as HTMLElement

      const handleTab = (e: KeyboardEvent) => {
        if (e.key === 'Tab') {
          if (e.shiftKey) {
            if (document.activeElement === firstElement) {
              e.preventDefault()
              lastElement?.focus()
            }
          } else {
            if (document.activeElement === lastElement) {
              e.preventDefault()
              firstElement?.focus()
            }
          }
        }
      }

      const handleEscape = (e: KeyboardEvent) => {
        if (e.key === 'Escape') {
          onClose()
        }
      }

      document.addEventListener('keydown', handleTab)
      document.addEventListener('keydown', handleEscape)
      document.body.style.overflow = 'hidden'

      return () => {
        document.removeEventListener('keydown', handleTab)
        document.removeEventListener('keydown', handleEscape)
        document.body.style.overflow = ''
        previousActiveElement.current?.focus()
      }
    }
  }, [isOpen, onClose])

  if (!isOpen) return null

  return createPortal(
    <div
      className="modal-overlay"
      onClick={onClose}
      role="presentation"
    >
      <div
        ref={dialogRef}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="modal-title"
        onClick={(e) => e.stopPropagation()}
        tabIndex={-1}
      >
        <h2 id="modal-title">{title}</h2>
        {children}
        <button onClick={onClose} aria-label="Close modal">
          Close
        </button>
      </div>
    </div>,
    document.body
  )
}
```

#### Accessible Form with Live Validation

```typescript
// src/components/AccessibleForm.tsx
import { useState, useId } from 'react'

export function AccessibleForm() {
  const [email, setEmail] = useState('')
  const [emailError, setEmailError] = useState('')
  const emailId = useId()
  const errorId = useId()

  const validateEmail = (value: string) => {
    if (!value) {
      setEmailError('Email is required')
      return false
    }
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value)) {
      setEmailError('Please enter a valid email address')
      return false
    }
    setEmailError('')
    return true
  }

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault()
        if (validateEmail(email)) {
          // Submit form
        }
      }}
      noValidate
    >
      <div>
        <label htmlFor={emailId}>
          Email Address
          <span aria-label="required">*</span>
        </label>
        <input
          id={emailId}
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          onBlur={(e) => validateEmail(e.target.value)}
          aria-invalid={!!emailError}
          aria-describedby={emailError ? errorId : undefined}
          aria-required="true"
        />
        {emailError && (
          <span id={errorId} role="alert" className="error">
            {emailError}
          </span>
        )}
      </div>
      <button type="submit">Submit</button>
    </form>
  )
}
```

#### Accessible Button with Loading State

```typescript
// src/components/AccessibleButton.tsx
import { ComponentPropsWithoutRef } from 'react'

interface AccessibleButtonProps extends ComponentPropsWithoutRef<'button'> {
  isLoading?: boolean
  loadingText?: string
  ref?: React.Ref<HTMLButtonElement>
}

// React 19: ref is a plain prop — no forwardRef, no displayName.
export function AccessibleButton({
  ref,
  isLoading,
  loadingText = 'Loading',
  children,
  ...props
}: AccessibleButtonProps) {
  return (
    <button
      ref={ref}
      disabled={isLoading || props.disabled}
      aria-busy={isLoading}
      aria-live="polite"
      {...props}
    >
      {isLoading ? (
        <>
          <span className="visually-hidden">{loadingText}</span>
          <span aria-hidden="true">
            <svg className="animate-spin" />
            {loadingText}
          </span>
        </>
      ) : (
        children
      )}
    </button>
  )
}
```

#### Skip to Content Link

```typescript
// src/components/SkipLink.tsx
export function SkipLink() {
  return (
    <a
      href="#main-content"
      className="skip-link"
      style={{
        position: 'absolute',
        left: '-10000px',
        top: 'auto',
        width: '1px',
        height: '1px',
        overflow: 'hidden',
      }}
      onFocus={(e) => {
        e.currentTarget.style.left = '0'
        e.currentTarget.style.width = 'auto'
        e.currentTarget.style.height = 'auto'
      }}
      onBlur={(e) => {
        e.currentTarget.style.left = '-10000px'
        e.currentTarget.style.width = '1px'
        e.currentTarget.style.height = '1px'
      }}
    >
      Skip to main content
    </a>
  )
}

// Usage in layout
export function Layout({ children }: { children: ReactNode }) {
  return (
    <div>
      <SkipLink />
      <header>
        <nav>...</nav>
      </header>
      <main id="main-content" tabIndex={-1}>
        {children}
      </main>
    </div>
  )
}
```

#### Accessible Dropdown Menu

```typescript
// src/components/DropdownMenu.tsx
import { useState, useRef, useEffect, useId } from 'react'

interface DropdownMenuProps {
  trigger: React.ReactNode
  items: Array<{ label: string; onClick: () => void }>
}

export function DropdownMenu({ trigger, items }: DropdownMenuProps) {
  const [isOpen, setIsOpen] = useState(false)
  const menuRef = useRef<HTMLUListElement>(null)
  const buttonRef = useRef<HTMLButtonElement>(null)
  const menuId = useId()

  useEffect(() => {
    if (isOpen && menuRef.current) {
      const firstItem = menuRef.current.querySelector('button') as HTMLButtonElement
      firstItem?.focus()
    }
  }, [isOpen])

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!isOpen) {
      if (e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowDown') {
        e.preventDefault()
        setIsOpen(true)
      }
      return
    }

    switch (e.key) {
      case 'Escape':
        setIsOpen(false)
        buttonRef.current?.focus()
        break
      case 'ArrowDown':
        e.preventDefault()
        const nextItem = (document.activeElement?.nextElementSibling as HTMLElement)
        nextItem?.querySelector('button')?.focus()
        break
      case 'ArrowUp':
        e.preventDefault()
        const prevItem = (document.activeElement?.previousElementSibling as HTMLElement)
        prevItem?.querySelector('button')?.focus()
        break
    }
  }

  return (
    <div className="dropdown">
      <button
        ref={buttonRef}
        aria-haspopup="true"
        aria-expanded={isOpen}
        aria-controls={menuId}
        onClick={() => setIsOpen(!isOpen)}
        onKeyDown={handleKeyDown}
      >
        {trigger}
      </button>

      {isOpen && (
        <ul
          ref={menuRef}
          id={menuId}
          role="menu"
          className="dropdown-menu"
        >
          {items.map((item, index) => (
            <li key={index} role="none">
              <button
                role="menuitem"
                onClick={() => {
                  item.onClick()
                  setIsOpen(false)
                  buttonRef.current?.focus()
                }}
                onKeyDown={handleKeyDown}
              >
                {item.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
```

#### Live Region for Announcements

```typescript
// src/components/LiveRegion.tsx
import { createContext, useContext, useState, ReactNode } from 'react'
import { createPortal } from 'react-dom'

interface LiveRegionContextValue {
  announce: (message: string, priority?: 'polite' | 'assertive') => void
}

const LiveRegionContext = createContext<LiveRegionContextValue | undefined>(
  undefined
)

export function useLiveRegion() {
  const context = useContext(LiveRegionContext)
  if (!context) {
    throw new Error('useLiveRegion must be used within LiveRegionProvider')
  }
  return context
}

export function LiveRegionProvider({ children }: { children: ReactNode }) {
  const [message, setMessage] = useState('')
  const [priority, setPriority] = useState<'polite' | 'assertive'>('polite')

  const announce = (
    newMessage: string,
    newPriority: 'polite' | 'assertive' = 'polite'
  ) => {
    setMessage('')
    setTimeout(() => {
      setMessage(newMessage)
      setPriority(newPriority)
    }, 100)
  }

  return (
    <LiveRegionContext.Provider value={{ announce }}>
      {children}
      {createPortal(
        <div
          role="status"
          aria-live={priority}
          aria-atomic="true"
          className="visually-hidden"
        >
          {message}
        </div>,
        document.body
      )}
    </LiveRegionContext.Provider>
  )
}

// Usage
function SaveButton() {
  const { announce } = useLiveRegion()

  const handleSave = async () => {
    try {
      await saveData()
      announce('Data saved successfully', 'polite')
    } catch (error) {
      announce('Failed to save data', 'assertive')
    }
  }

  return <button onClick={handleSave}>Save</button>
}
```

#### Visually Hidden Utility

```css
/* src/styles/utilities.css */
.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border-width: 0;
}
```

## Anti-Patterns

### FORBIDDEN: Never Use These

```typescript
// FORBIDDEN: Next.js (this is an SPA skill, not SSR)
// DO NOT use Next.js, App Router, Server Components from Next.js
// DO NOT use: next/navigation, next/router, next/link, etc.

// FORBIDDEN: Remix (this is an SPA skill)
// DO NOT use Remix framework

// FORBIDDEN: create-react-app
// ALWAYS use Vite for new projects
// CRA is deprecated and unmaintained

// FORBIDDEN: webpack directly
// ALWAYS use Vite as the bundler
// DO NOT create custom webpack configs

// FORBIDDEN: Redux (when Jotai can be used)
// DO NOT use Redux, Redux Toolkit, or React-Redux
// ONLY use Jotai for global state management
// Exception: Existing projects already using Redux

// FORBIDDEN: Context API for global state
// DO NOT use Context + useContext for application state
// Context is fine for component-level state (themes, etc.)
// Use Jotai atoms for all global application state
```

### Common Mistakes to Avoid

```typescript
// BAD: Mutation instead of immutability
const [items, setItems] = useState<Item[]>([])
items.push(newItem) // Direct mutation
setItems(items) // React won't detect the change

// GOOD: Immutable updates
setItems([...items, newItem])
setItems((prev) => [...prev, newItem])

// BAD: Missing dependency in useEffect
useEffect(() => {
  fetchData(userId)
}, []) // userId not in deps

// GOOD: Include all dependencies
useEffect(() => {
  fetchData(userId)
}, [userId])

// BAD: Derived state that should be computed
const [items, setItems] = useState<Item[]>([])
const [filteredItems, setFilteredItems] = useState<Item[]>([])

useEffect(() => {
  setFilteredItems(items.filter(filter))
}, [items, filter])

// GOOD: Compute during render
const filteredItems = items.filter(filter)

// BAD: Prop drilling through many levels
function App() {
  const [user, setUser] = useState<User>()
  return <Level1 user={user} setUser={setUser} />
}

// GOOD: Use Jotai for shared state
const userAtom = atom<User | null>(null)

function App() {
  return <Level1 />
}

function DeepChild() {
  const [user, setUser] = useAtom(userAtom)
  // Direct access without prop drilling
}

// BAD: Adding useCallback when identity does not matter
function Parent() {
  const handleClick = useCallback(() => {
    doSomething()
  }, [])

  return <button onClick={handleClick}>Save</button>
}

// GOOD: Inline handlers are fine unless a memoized child or Effect needs stability
function Parent() {
  return <button onClick={() => doSomething()}>Save</button>
}

// BAD: Fetching in components without Suspense
function UserProfile({ userId }: { userId: string }) {
  const [user, setUser] = useState<User | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    fetchUser(userId).then(setUser).finally(() => setLoading(false))
  }, [userId])

  if (loading) return <div>Loading...</div>
  // ...
}

// GOOD: Use Suspense with async atoms
const userAtomFamily = atomFamily((userId: string) =>
  atom(async () => {
    const response = await fetch(`/api/users/${userId}`)
    return response.json()
  })
)

function UserProfile({ userId }: { userId: string }) {
  const user = useAtomValue(userAtomFamily(userId))
  return <div>{user.name}</div>
}

function UserContainer({ userId }: { userId: string }) {
  return (
    <Suspense fallback={<div>Loading...</div>}>
      <UserProfile userId={userId} />
    </Suspense>
  )
}
```

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

These pairs are the defaults Loom harnesses should follow when choosing between refs, state, effects, memoization, and shared state.

```typescript
// BAD: Using a ref for UI state just to avoid re-renders
function Tabs() {
  const selectedTabRef = useRef("settings")

  return <Panel tab={selectedTabRef.current} />
}

// GOOD: Use state for values that affect rendering
function Tabs() {
  const [selectedTab, setSelectedTab] = useState("settings")

  return <Panel tab={selectedTab} onSelect={setSelectedTab} />
}

// BAD: Deriving render state inside an Effect
function Profile({ firstName, lastName }: { firstName: string; lastName: string }) {
  const [fullName, setFullName] = useState("")

  useEffect(() => {
    setFullName(`${firstName} ${lastName}`)
  }, [firstName, lastName])

  return <h1>{fullName}</h1>
}

// GOOD: Derive during render; reserve Effects for external synchronization
function Profile({ firstName, lastName }: { firstName: string; lastName: string }) {
  const fullName = `${firstName} ${lastName}`
  return <h1>{fullName}</h1>
}

// BAD: Syncing a ref to keep "latest" values inside an Effect callback
function ChatRoom({ roomId, theme }: { roomId: string; theme: Theme }) {
  const latestTheme = useRef(theme)

  useEffect(() => {
    latestTheme.current = theme
  })

  useEffect(() => {
    const connection = createConnection(roomId)
    connection.on("connected", () => {
      showToast("Connected", latestTheme.current)
    })
    connection.connect()

    return () => connection.disconnect()
  }, [roomId])
}

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

// BAD: Wrapping every handler in useCallback by default
function SaveButton() {
  const handleClick = useCallback(() => {
    saveDraft()
  }, [])

  return <button onClick={handleClick}>Save</button>
}

// GOOD: Use useCallback when a memoized child or Effect depends on identity
const ToolbarButton = memo(function ToolbarButton({
  onClick,
}: {
  onClick: () => void
}) {
  return <button onClick={onClick}>Save</button>
})

function SaveButton() {
  const handleClick = useCallback(() => {
    saveDraft()
  }, [])

  return <ToolbarButton onClick={handleClick} />
}

// BAD: Memoizing cheap values "just in case"
function Summary({ items }: { items: Item[] }) {
  const count = useMemo(() => items.length, [items])
  return <span>{count}</span>
}

// GOOD: Use useMemo for expensive derived values or stable props for memoized children
function SearchResults({ items, query }: { items: Item[]; query: string }) {
  const visibleItems = useMemo(() => {
    return filterLargeList(items, query)
  }, [items, query])

  return <ExpensiveList items={visibleItems} />
}

// BAD: A giant Context for fast-changing application state
const AppContext = createContext<AppState | null>(null)

function AppProviders({ children }: { children: ReactNode }) {
  return (
    <AppContext value={{ theme, session, filters, draft, modals }}>
      {children}
    </AppContext>
  )
}

// GOOD: Keep Context for ambient subtree values; use Jotai for hot app state
const ThemeContext = createContext<Theme>("light")

function AppProviders({ children }: { children: ReactNode }) {
  return <ThemeContext value={theme}>{children}</ThemeContext>
}

const draftAtom = atom("")

// BAD: Many related useState calls plus sync logic between them
function Form() {
  const [status, setStatus] = useState<"idle" | "saving" | "error">("idle")
  const [data, setData] = useState<FormData>({ name: "" })
  const [error, setError] = useState<string | null>(null)
  // More setters and cross-field updates...
}

// GOOD: Use useReducer when transitions are related and explicit
type FormState = {
  status: "idle" | "saving" | "error"
  data: FormData
  error: string | null
}

type FormAction =
  | { type: "changed_name"; value: string }
  | { type: "save_started" }
  | { type: "save_failed"; message: string }
  | { type: "save_succeeded" }

function formReducer(state: FormState, action: FormAction): FormState {
  switch (action.type) {
    case "changed_name":
      return {
        ...state,
        data: { ...state.data, name: action.value },
      }
    case "save_started":
      return { ...state, status: "saving", error: null }
    case "save_failed":
      return { ...state, status: "error", error: action.message }
    case "save_succeeded":
      return { ...state, status: "idle", error: null }
    default:
      throw new Error(`Unknown action: ${JSON.stringify(action)}`)
  }
}

function Form() {
  const [state, dispatch] = useReducer(formReducer, {
    status: "idle",
    data: { name: "" },
    error: null,
  })

  return (
    <input
      value={state.data.name}
      onChange={(e) =>
        dispatch({ type: "changed_name", value: e.target.value })
      }
    />
  )
}

// BAD: setInterval-driven animation that ignores reduced-motion preferences
useEffect(() => {
  const id = window.setInterval(() => {
    advanceSpinnerFrame()
  }, 16)

  return () => window.clearInterval(id)
}, [])

// GOOD: Use requestAnimationFrame for visual work and respect prefers-reduced-motion
useEffect(() => {
  const mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)")
  if (mediaQuery.matches) return

  let frame = 0

  const tick = (time: number) => {
    drawFrame(time)
    frame = window.requestAnimationFrame(tick)
  }

  frame = window.requestAnimationFrame(tick)

  return () => window.cancelAnimationFrame(frame)
}, [])
```

### DO NOT Use

- **Next.js** - Use for SSR projects, not SPAs
- **Remix** - Use for full-stack projects, not SPAs
- **Redux** - Use Jotai instead for simpler, more atomic state
- **Context API for global state** - Use Jotai atoms instead
- **create-react-app** - Deprecated, use Vite
- **webpack** - Use Vite bundler
- **Class components** - Use function components with hooks
- **Default exports** - Prefer named exports for better refactoring

## Testing

### Component Testing with Vitest

```typescript
// vite.config.ts - Add test configuration
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
  },
})

// src/test/setup.ts
import { expect, afterEach } from 'vitest'
import { cleanup } from '@testing-library/react'
import * as matchers from '@testing-library/jest-dom/matchers'

expect.extend(matchers)

afterEach(() => {
  cleanup()
})

// src/components/Button.test.tsx
import { render, screen, fireEvent } from '@testing-library/react'
import { describe, it, expect, vi } from 'vitest'
import { Button } from './Button'

describe('Button', () => {
  it('renders children correctly', () => {
    render(<Button>Click me</Button>)
    expect(screen.getByRole('button', { name: /click me/i })).toBeInTheDocument()
  })

  it('calls onClick when clicked', () => {
    const handleClick = vi.fn()
    render(<Button onClick={handleClick}>Click me</Button>)

    fireEvent.click(screen.getByRole('button'))
    expect(handleClick).toHaveBeenCalledTimes(1)
  })

  it('is disabled when isLoading is true', () => {
    render(<Button isLoading>Click me</Button>)
    expect(screen.getByRole('button')).toBeDisabled()
  })
})
```

### Testing with Jotai

```typescript
// src/store/counter.test.ts
import { renderHook, act } from "@testing-library/react";
import { useAtom } from "jotai";
import { describe, it, expect } from "vitest";
import { countAtom, incrementAtom } from "./counter";

describe("counter atoms", () => {
  it("increments count", () => {
    const { result } = renderHook(() => ({
      count: useAtom(countAtom),
      increment: useAtom(incrementAtom),
    }));

    expect(result.current.count[0]).toBe(0);

    act(() => {
      result.current.increment[1]();
    });

    expect(result.current.count[0]).toBe(1);
  });
});
```

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

This React skill provides comprehensive guidance for building modern SPAs with React Router, Jotai, and Vite, while explicitly avoiding Next.js, Redux, and other tools that don't fit the SPA paradigm.
