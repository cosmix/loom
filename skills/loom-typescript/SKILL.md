---
name: loom-typescript
description: TypeScript language expertise for type-safe, production-quality code. Use for advanced type system features (generics, discriminated unions, conditional and mapped types), strict mode configuration, type-safe APIs with zod/trpc/prisma, and modern tooling across Node, Deno, and Bun.
triggers:
  - typescript
  - ts
  - tsx
  - type
  - interface
  - generic
  - union
  - intersection
  - discriminated union
  - type guard
  - type assertion
  - utility types
  - conditional types
  - mapped types
  - zod
  - trpc
  - prisma
  - react
  - node
  - nodejs
  - deno
  - bun
  - npm
  - pnpm
  - yarn
  - type-safe
  - type safety
  - tsconfig
  - strict mode
  - branded types
---

# TypeScript Language Expertise

## Overview

This skill provides guidance for writing type-safe, maintainable, and production-quality TypeScript code. It covers TypeScript's advanced type system features, strict mode configuration, module systems, and common design patterns.

## Key Concepts

### Generics

```typescript
// Basic generics
function identity<T>(value: T): T {
  return value;
}

// Multiple type parameters
function map<T, U>(items: T[], fn: (item: T) => U): U[] {
  return items.map(fn);
}

// Generic constraints
interface HasLength {
  length: number;
}

function logLength<T extends HasLength>(item: T): void {
  console.log(item.length);
}

// Generic classes
class Repository<T extends { id: string }> {
  private items: Map<string, T> = new Map();

  save(item: T): void {
    this.items.set(item.id, item);
  }

  findById(id: string): T | undefined {
    return this.items.get(id);
  }

  findAll(): T[] {
    return Array.from(this.items.values());
  }
}

// Default type parameters
interface ApiResponse<T = unknown> {
  data: T;
  status: number;
  message: string;
}
```

### Utility Types

```typescript
// Built-in utility types
interface User {
  id: string;
  email: string;
  name: string;
  role: "admin" | "user";
  createdAt: Date;
}

// Partial - all properties optional
type UserUpdate = Partial<User>;

// Required - all properties required
type RequiredUser = Required<User>;

// Readonly - all properties readonly
type ImmutableUser = Readonly<User>;

// Pick - select specific properties
type UserCredentials = Pick<User, "email" | "id">;

// Omit - exclude specific properties
type UserWithoutDates = Omit<User, "createdAt">;

// Record - create object type with specific keys
type UserRoles = Record<string, "admin" | "user" | "guest">;

// Extract/Exclude for union types
type StringOrNumber = string | number | boolean;
type OnlyStrings = Extract<StringOrNumber, string>; // string
type NoStrings = Exclude<StringOrNumber, string>; // number | boolean

// ReturnType and Parameters
function createUser(name: string, email: string): User {
  return {
    id: crypto.randomUUID(),
    name,
    email,
    role: "user",
    createdAt: new Date(),
  };
}

type CreateUserReturn = ReturnType<typeof createUser>; // User
type CreateUserParams = Parameters<typeof createUser>; // [string, string]

// NonNullable
type MaybeString = string | null | undefined;
type DefiniteString = NonNullable<MaybeString>; // string
```

### Conditional Types

```typescript
// Basic conditional type
type IsString<T> = T extends string ? true : false;

// Infer keyword for type extraction
type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
type UnwrapArray<T> = T extends (infer U)[] ? U : T;

// Nested inference
type GetReturnType<T> = T extends (...args: any[]) => infer R ? R : never;

// Distributive conditional types
type ToArray<T> = T extends any ? T[] : never;
type StringOrNumberArray = ToArray<string | number>; // string[] | number[]

// Non-distributive conditional types
type ToArrayNonDist<T> = [T] extends [any] ? T[] : never;
type Combined = ToArrayNonDist<string | number>; // (string | number)[]

// Practical example: Extract function parameters
type FirstParameter<T> = T extends (first: infer F, ...args: any[]) => any
  ? F
  : never;
```

### Mapped Types

```typescript
// Basic mapped type
type Nullable<T> = {
  [K in keyof T]: T[K] | null;
};

// With modifiers
type Mutable<T> = {
  -readonly [K in keyof T]: T[K];
};

type Optional<T> = {
  [K in keyof T]+?: T[K];
};

// Key remapping (TypeScript 4.1+)
type Getters<T> = {
  [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};

type Setters<T> = {
  [K in keyof T as `set${Capitalize<string & K>}`]: (value: T[K]) => void;
};

// Filter keys
type FilterByType<T, U> = {
  [K in keyof T as T[K] extends U ? K : never]: T[K];
};

interface Mixed {
  name: string;
  age: number;
  active: boolean;
  email: string;
}

type StringProps = FilterByType<Mixed, string>; // { name: string; email: string }

// Practical: API response transformation
type ApiDTO<T> = {
  [K in keyof T as `${string & K}DTO`]: T[K] extends Date ? string : T[K];
};
```

### Discriminated Unions

```typescript
// Define discriminated union with literal type discriminator
type Result<T, E = Error> =
  | { success: true; data: T }
  | { success: false; error: E };

function handleResult<T>(result: Result<T>): T {
  if (result.success) {
    return result.data; // TypeScript knows data exists here
  }
  throw result.error; // TypeScript knows error exists here
}

// More complex example: State machine
type LoadingState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "success"; data: User[] }
  | { status: "error"; error: Error };

function renderState(state: LoadingState): string {
  switch (state.status) {
    case "idle":
      return "Click to load";
    case "loading":
      return "Loading...";
    case "success":
      return `Loaded ${state.data.length} users`;
    case "error":
      return `Error: ${state.error.message}`;
  }
}

// Action types for Redux-style reducers
type Action =
  | { type: "SET_USER"; payload: User }
  | { type: "CLEAR_USER" }
  | { type: "SET_ERROR"; payload: string };

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "SET_USER":
      return { ...state, user: action.payload };
    case "CLEAR_USER":
      return { ...state, user: null };
    case "SET_ERROR":
      return { ...state, error: action.payload };
  }
}
```

### Type Guards

```typescript
// typeof guard
function process(value: string | number): string {
  if (typeof value === "string") {
    return value.toUpperCase();
  }
  return value.toFixed(2);
}

// instanceof guard
function handleError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

// Custom type guard
interface Cat {
  meow(): void;
}

interface Dog {
  bark(): void;
}

function isCat(animal: Cat | Dog): animal is Cat {
  return "meow" in animal;
}

// Type guard with discriminated unions
function isSuccess<T>(result: Result<T>): result is { success: true; data: T } {
  return result.success;
}

// Assertion function
function assertNonNull<T>(
  value: T | null | undefined,
  message?: string,
): asserts value is T {
  if (value === null || value === undefined) {
    throw new Error(message ?? "Value is null or undefined");
  }
}

// Usage
function processUser(user: User | null) {
  assertNonNull(user, "User must exist");
  // user is now User (not null)
  console.log(user.name);
}
```

## Best Practices

### Strict Mode Configuration

The `module`/`moduleResolution` pair must match where the output runs — there is no single universal template. Use one of the two configs below; do NOT copy a `nodenext` config into a bundler app or vice versa.

**Node app or published npm library** — `module: nodenext` (canonical lowercase). This requires explicit `.js` extensions on relative imports plus `"type": "module"` in `package.json` (or `.mts`/`.cts` files). `nodenext` implies a matching `lib`/`target`, so the explicit `"lib"` is redundant and dropped here.

```json
{
  "compilerOptions": {
    "module": "nodenext",
    "verbatimModuleSyntax": true,
    "outDir": "./dist",
    "rootDir": "./src",
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,

    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noImplicitReturns": true,
    "noFallthroughCasesInSwitch": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,

    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
```

**Bundler app (Vite, esbuild, webpack)** — extensionless relative imports work; the bundler does emit, so `tsc` only type-checks. Never use `moduleResolution: bundler` for a published library: it is "infectious" and emits `.d.ts` files with extensionless relative imports that break Node.js ESM consumers.

```json
{
  "compilerOptions": {
    "module": "esnext",
    "moduleResolution": "bundler",
    "verbatimModuleSyntax": true,
    "noEmit": true,

    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noImplicitReturns": true,
    "noFallthroughCasesInSwitch": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,

    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true
  },
  "include": ["src/**/*"]
}
```

> **`noUncheckedIndexedAccess` and `exactOptionalPropertyTypes` are NOT part of `strict: true`** — opt in explicitly (both shown above; TS 5.9 `tsc --init` enables them by default for new projects). `noUncheckedIndexedAccess` adds `| undefined` to every array subscript and index-signature access (`arr[i]` becomes `T | undefined`), but NOT to named properties, NOT to `for...of` loop variables (closed as "working as intended", microsoft/TypeScript#42622), and NOT to `Object.values()` — so it is no safety net when iterating. `exactOptionalPropertyTypes` makes `obj.x = undefined` an error for `x?: 'a' | 'b'` (only deleting the key makes the property absent), which matters for `'x' in obj` checks and serialization. `verbatimModuleSyntax` is the modern module-safety baseline that supersedes the now-no-op `importsNotUsedAsValues`/`preserveValueImports` (see Module Organization).

### Module Organization

```typescript
// Re-export pattern for clean public API
// src/models/index.ts
export { User, type UserDTO } from "./user";
export { Order, type OrderDTO } from "./order";
export { Product, type ProductDTO } from "./product";

// Barrel exports with explicit types
// src/index.ts
export type { Config, ConfigOptions } from "./config";
export { createConfig, validateConfig } from "./config";

// Namespace imports for related utilities
import * as validators from "./validators";
import * as formatters from "./formatters";

// Type-only imports
import type { User, Order } from "./models";
import { createUser, createOrder } from "./models";

// Under verbatimModuleSyntax: true, every type-only specifier mixed into a
// value import/export MUST carry the inline `type` modifier — otherwise it is
// emitted as a runtime import even when used only as a type.
import { type User, createUser } from "./user"; // User erased, createUser kept
export { Order, type OrderDTO } from "./order"; // OrderDTO erased
```

### Declaration Files

```typescript
// global.d.ts - Extend global types
declare global {
  interface Window {
    analytics: AnalyticsAPI;
  }

  namespace NodeJS {
    interface ProcessEnv {
      NODE_ENV: "development" | "production" | "test";
      DATABASE_URL: string;
      API_KEY: string;
    }
  }
}

// module.d.ts - Declare untyped modules
declare module "untyped-package" {
  export function doSomething(value: string): void;
  export const VERSION: string;
}

// Augment existing modules
declare module "express" {
  interface Request {
    user?: User;
    requestId: string;
  }
}

export {}; // Makes this a module
```

## Common Patterns

### Branded Types

```typescript
// Create nominal types for type safety
declare const brand: unique symbol;

type Brand<T, B> = T & { [brand]: B };

type UserId = Brand<string, "UserId">;
type OrderId = Brand<string, "OrderId">;
type Email = Brand<string, "Email">;

// Constructor functions with validation
function createUserId(id: string): UserId {
  if (!id.match(/^usr_[a-z0-9]+$/)) {
    throw new Error("Invalid user ID format");
  }
  return id as UserId;
}

function createEmail(email: string): Email {
  if (!email.includes("@")) {
    throw new Error("Invalid email format");
  }
  return email.toLowerCase() as Email;
}

// Now these can't be accidentally mixed
function getUser(id: UserId): Promise<User> {
  /* ... */
}
function getOrder(id: OrderId): Promise<Order> {
  /* ... */
}

// const userId = createUserId('usr_123');
// const orderId = createOrderId('ord_456');
// getUser(orderId); // Type error!
```

### Builder Pattern

```typescript
class QueryBuilder<T extends object> {
  private filters: Partial<T> = {};
  private sortField?: keyof T;
  private sortOrder: "asc" | "desc" = "asc";
  private limitValue?: number;
  private offsetValue?: number;

  where<K extends keyof T>(field: K, value: T[K]): this {
    this.filters[field] = value;
    return this;
  }

  orderBy(field: keyof T, order: "asc" | "desc" = "asc"): this {
    this.sortField = field;
    this.sortOrder = order;
    return this;
  }

  limit(value: number): this {
    this.limitValue = value;
    return this;
  }

  offset(value: number): this {
    this.offsetValue = value;
    return this;
  }

  build(): Query<T> {
    return {
      filters: this.filters,
      sort: this.sortField
        ? { field: this.sortField, order: this.sortOrder }
        : undefined,
      pagination: { limit: this.limitValue, offset: this.offsetValue },
    };
  }
}

// Usage with type inference
const query = new QueryBuilder<User>()
  .where("role", "admin")
  .orderBy("createdAt", "desc")
  .limit(10)
  .build();
```

### Exhaustive Checks

```typescript
// Ensure all union cases are handled
function assertNever(value: never): never {
  throw new Error(`Unexpected value: ${value}`);
}

type Status = "pending" | "approved" | "rejected" | "cancelled";

function getStatusColor(status: Status): string {
  switch (status) {
    case "pending":
      return "yellow";
    case "approved":
      return "green";
    case "rejected":
      return "red";
    case "cancelled":
      return "gray";
    default:
      return assertNever(status); // Compile error if case is missing
  }
}

// With discriminated unions
type Event =
  | { type: "click"; x: number; y: number }
  | { type: "keypress"; key: string }
  | { type: "scroll"; delta: number };

function handleEvent(event: Event): void {
  switch (event.type) {
    case "click":
      console.log(`Clicked at ${event.x}, ${event.y}`);
      break;
    case "keypress":
      console.log(`Key pressed: ${event.key}`);
      break;
    case "scroll":
      console.log(`Scrolled: ${event.delta}`);
      break;
    default:
      assertNever(event);
  }
}
```

### Type-Safe Event Emitter

```typescript
type EventMap = {
  userCreated: { user: User };
  userDeleted: { userId: string };
  orderPlaced: { order: Order; user: User };
};

class TypedEventEmitter<T extends Record<string, any>> {
  private listeners: { [K in keyof T]?: Array<(payload: T[K]) => void> } = {};

  on<K extends keyof T>(
    event: K,
    listener: (payload: T[K]) => void,
  ): () => void {
    if (!this.listeners[event]) {
      this.listeners[event] = [];
    }
    this.listeners[event]!.push(listener);

    return () => this.off(event, listener);
  }

  off<K extends keyof T>(event: K, listener: (payload: T[K]) => void): void {
    const listeners = this.listeners[event];
    if (listeners) {
      const index = listeners.indexOf(listener);
      if (index !== -1) {
        listeners.splice(index, 1);
      }
    }
  }

  emit<K extends keyof T>(event: K, payload: T[K]): void {
    this.listeners[event]?.forEach((listener) => listener(payload));
  }
}

// Usage
const emitter = new TypedEventEmitter<EventMap>();

emitter.on("userCreated", ({ user }) => {
  console.log(`User created: ${user.name}`);
});

emitter.emit("userCreated", { user: newUser });
// emitter.emit('userCreated', { wrong: 'payload' }); // Type error!
```

## Type-Safe API Patterns

### Zod for Runtime Validation

```typescript
import { z } from "zod";

// Define schemas that generate both runtime validators and static types
const UserSchema = z.object({
  id: z.string().uuid(),
  email: z.string().email(),
  name: z.string().min(1).max(100),
  age: z.number().int().positive().optional(),
  role: z.enum(["admin", "user", "guest"]).default("user"),
  createdAt: z.coerce.date(),
  metadata: z.record(z.string(), z.unknown()),
});

// Extract TypeScript type from schema
type User = z.infer<typeof UserSchema>;

// Nested schemas
const OrderSchema = z.object({
  id: z.string(),
  user: UserSchema,
  items: z.array(
    z.object({
      productId: z.string(),
      quantity: z.number().positive(),
      price: z.number().positive(),
    }),
  ),
  total: z.number().positive(),
  status: z.enum(["pending", "paid", "shipped", "delivered"]),
});

type Order = z.infer<typeof OrderSchema>;

// Parse with error handling
function createUser(input: unknown): User {
  return UserSchema.parse(input); // Throws ZodError on validation failure
}

// Safe parse returns result object
function createUserSafe(input: unknown): Result<User, z.ZodError> {
  const result = UserSchema.safeParse(input);
  if (result.success) {
    return { success: true, data: result.data };
  }
  return { success: false, error: result.error };
}

// Transform and refine
const PasswordSchema = z
  .string()
  .min(8)
  .regex(/[A-Z]/, "Must contain uppercase")
  .regex(/[a-z]/, "Must contain lowercase")
  .regex(/[0-9]/, "Must contain number");

const SignupSchema = z
  .object({
    email: z.string().email(),
    password: PasswordSchema,
    confirmPassword: z.string(),
  })
  .refine((data) => data.password === data.confirmPassword, {
    message: "Passwords must match",
    path: ["confirmPassword"],
  });

// Partial, pick, omit on schemas
const UserUpdateSchema = UserSchema.partial(); // All fields optional
const UserCredentialsSchema = UserSchema.pick({ email: true, id: true });
const UserWithoutDatesSchema = UserSchema.omit({ createdAt: true });
```

### tRPC for End-to-End Type Safety

```typescript
import { initTRPC } from "@trpc/server";
import { z } from "zod";

// Initialize tRPC
const t = initTRPC.context<Context>().create();

// Define router with typed procedures
const appRouter = t.router({
  // Query with input validation
  getUser: t.procedure
    .input(z.object({ id: z.string().uuid() }))
    .query(async ({ input, ctx }) => {
      const user = await ctx.db.user.findUnique({
        where: { id: input.id },
      });
      if (!user) throw new TRPCError({ code: "NOT_FOUND" });
      return user;
    }),

  // Mutation with input validation
  createUser: t.procedure
    .input(UserSchema.omit({ id: true, createdAt: true }))
    .mutation(async ({ input, ctx }) => {
      return await ctx.db.user.create({ data: input });
    }),

  // Protected procedure with middleware
  updateProfile: t.procedure
    .use(isAuthenticated)
    .input(UserSchema.partial().required({ id: true }))
    .mutation(async ({ input, ctx }) => {
      return await ctx.db.user.update({
        where: { id: input.id },
        data: input,
      });
    }),

  // Nested routers
  posts: t.router({
    list: t.procedure
      .input(
        z.object({
          limit: z.number().min(1).max(100).default(10),
          cursor: z.string().optional(),
        })
      )
      .query(async ({ input }) => {
        // Returns typed data
        return { posts: [], nextCursor: null };
      }),

    byId: t.procedure.input(z.string()).query(async ({ input }) => {
      // input is string
      return { id: input, title: "Post" };
    }),
  }),
});

// Export type for client
export type AppRouter = typeof appRouter;

// Client usage (in separate file)
import { createTRPCClient } from "@trpc/client";
import type { AppRouter } from "./server";

const client = createTRPCClient<AppRouter>({
  url: "http://localhost:3000/trpc",
});

// Fully typed, autocomplete works
const user = await client.getUser.query({ id: "uuid-here" });
// user is typed as User

const newUser = await client.createUser.mutate({
  email: "user@example.com",
  name: "John",
  role: "user",
});
// newUser is typed based on the mutation return

// React hook usage
import { trpc } from "./trpc";

function UserProfile({ userId }: { userId: string }) {
  const { data, isLoading } = trpc.getUser.useQuery({ id: userId });
  const updateMutation = trpc.updateProfile.useMutation();

  if (isLoading) return <div>Loading...</div>;
  return <div>{data.name}</div>;
}
```

### Prisma for Type-Safe Database Access

```typescript
import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();

// Generated types from schema.prisma
// All queries are fully typed

// Basic CRUD operations
async function createUser(email: string, name: string) {
  return await prisma.user.create({
    data: { email, name },
    // select/include are type-checked
    select: { id: true, email: true, name: true },
  });
}

// Relations are typed
async function getUserWithPosts(userId: string) {
  return await prisma.user.findUnique({
    where: { id: userId },
    include: {
      posts: {
        where: { published: true },
        orderBy: { createdAt: "desc" },
        take: 10,
      },
    },
  });
  // Return type includes User & { posts: Post[] }
}

// Type-safe where clauses
async function findUsers(filters: {
  role?: string;
  createdAfter?: Date;
  emailContains?: string;
}) {
  return await prisma.user.findMany({
    where: {
      role: filters.role,
      createdAt: { gte: filters.createdAfter },
      email: { contains: filters.emailContains },
    },
  });
}

// Transactions
async function transferCredits(fromId: string, toId: string, amount: number) {
  return await prisma.$transaction(async (tx) => {
    const from = await tx.user.update({
      where: { id: fromId },
      data: { credits: { decrement: amount } },
    });

    const to = await tx.user.update({
      where: { id: toId },
      data: { credits: { increment: amount } },
    });

    return { from, to };
  });
}

// Extending Prisma Client with custom methods
const xprisma = prisma.$extends({
  model: {
    user: {
      async findByEmail(email: string) {
        return await prisma.user.findUnique({ where: { email } });
      },
    },
  },
});
```

## React TypeScript Patterns

### Component Props and Generic Components

```typescript
import { ReactNode, ComponentPropsWithoutRef } from "react";

// Basic component with props interface
interface ButtonProps {
  variant: "primary" | "secondary" | "danger";
  size?: "sm" | "md" | "lg";
  disabled?: boolean;
  onClick?: () => void;
  children: ReactNode;
}

function Button({
  variant,
  size = "md",
  disabled,
  onClick,
  children,
}: ButtonProps) {
  return (
    <button
      className={`btn-${variant} btn-${size}`}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

// Extend native HTML element props
interface InputProps extends ComponentPropsWithoutRef<"input"> {
  label: string;
  error?: string;
}

function Input({ label, error, ...inputProps }: InputProps) {
  return (
    <div>
      <label>{label}</label>
      <input {...inputProps} aria-invalid={!!error} />
      {error && <span className="error">{error}</span>}
    </div>
  );
}

// Generic component for lists
interface ListProps<T> {
  items: T[];
  renderItem: (item: T, index: number) => ReactNode;
  keyExtractor: (item: T) => string | number;
  emptyMessage?: string;
}

function List<T>({
  items,
  renderItem,
  keyExtractor,
  emptyMessage,
}: ListProps<T>) {
  if (items.length === 0) {
    return <div>{emptyMessage || "No items"}</div>;
  }

  return (
    <ul>
      {items.map((item, index) => (
        <li key={keyExtractor(item)}>{renderItem(item, index)}</li>
      ))}
    </ul>
  );
}

// Usage with type inference
<List
  items={users}
  renderItem={(user) => <div>{user.name}</div>}
  keyExtractor={(user) => user.id}
/>;

// Polymorphic component (as prop pattern)
type AsProp<C extends React.ElementType> = {
  as?: C;
};

type PropsToOmit<C extends React.ElementType, P> = keyof (AsProp<C> & P);

type PolymorphicComponentProp<
  C extends React.ElementType,
  Props = {}
> = React.PropsWithChildren<Props & AsProp<C>> &
  Omit<React.ComponentPropsWithoutRef<C>, PropsToOmit<C, Props>>;

type TextProps<C extends React.ElementType> = PolymorphicComponentProp<
  C,
  {
    color?: "primary" | "secondary";
    size?: "sm" | "md" | "lg";
  }
>;

function Text<C extends React.ElementType = "span">({
  as,
  color = "primary",
  size = "md",
  children,
  ...props
}: TextProps<C>) {
  const Component = as || "span";
  return (
    <Component className={`text-${color} text-${size}`} {...props}>
      {children}
    </Component>
  );
}

// Usage
<Text>Default span</Text>;
<Text as="h1">Heading</Text>;
<Text as="a" href="/link">
  Link
</Text>;
```

### Hooks and State Management

```typescript
import { useState, useEffect, useCallback, useRef, useReducer } from "react";

// Typed useState
function Counter() {
  const [count, setCount] = useState(0);
  const [user, setUser] = useState<User | null>(null);

  // Type inference works
  setCount(count + 1);
  setUser({ id: "1", name: "John", email: "john@example.com" });
}

// Custom hooks with generic types
function useLocalStorage<T>(key: string, initialValue: T) {
  const [value, setValue] = useState<T>(() => {
    const stored = localStorage.getItem(key);
    return stored ? JSON.parse(stored) : initialValue;
  });

  useEffect(() => {
    localStorage.setItem(key, JSON.stringify(value));
  }, [key, value]);

  return [value, setValue] as const;
}

// Usage with type inference
const [user, setUser] = useLocalStorage<User | null>("user", null);

// useReducer with discriminated unions
type State = {
  status: "idle" | "loading" | "success" | "error";
  data: User | null;
  error: string | null;
};

type Action =
  | { type: "FETCH_START" }
  | { type: "FETCH_SUCCESS"; payload: User }
  | { type: "FETCH_ERROR"; payload: string };

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "FETCH_START":
      return { status: "loading", data: null, error: null };
    case "FETCH_SUCCESS":
      return { status: "success", data: action.payload, error: null };
    case "FETCH_ERROR":
      return { status: "error", data: null, error: action.payload };
  }
}

function useUser(userId: string) {
  const [state, dispatch] = useReducer(reducer, {
    status: "idle",
    data: null,
    error: null,
  });

  useEffect(() => {
    dispatch({ type: "FETCH_START" });
    fetchUser(userId)
      .then((user) => dispatch({ type: "FETCH_SUCCESS", payload: user }))
      .catch((error) =>
        dispatch({ type: "FETCH_ERROR", payload: error.message })
      );
  }, [userId]);

  return state;
}

// Ref with typed DOM elements
function VideoPlayer() {
  const videoRef = useRef<HTMLVideoElement>(null);

  const play = useCallback(() => {
    videoRef.current?.play();
  }, []);

  return <video ref={videoRef} />;
}
```

### Context API with TypeScript

```typescript
import { createContext, useContext, ReactNode } from "react";

// Define context value type
interface AuthContextValue {
  user: User | null;
  login: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  isLoading: boolean;
}

// Create context with undefined initial value
const AuthContext = createContext<AuthContextValue | undefined>(undefined);

// Provider component
interface AuthProviderProps {
  children: ReactNode;
}

function AuthProvider({ children }: AuthProviderProps) {
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const login = async (email: string, password: string) => {
    setIsLoading(true);
    const user = await api.login(email, password);
    setUser(user);
    setIsLoading(false);
  };

  const logout = async () => {
    await api.logout();
    setUser(null);
  };

  const value = { user, login, logout, isLoading };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

// Custom hook with runtime check
function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (context === undefined) {
    throw new Error("useAuth must be used within AuthProvider");
  }
  return context;
}

// Usage in components
function Profile() {
  const { user, logout } = useAuth(); // Fully typed

  if (!user) return <div>Not logged in</div>;

  return (
    <div>
      <h1>{user.name}</h1>
      <button onClick={logout}>Logout</button>
    </div>
  );
}
```

## Node.js TypeScript Patterns

### Express with Type Safety

```typescript
import express, { Request, Response, NextFunction } from "express";
import { z } from "zod";

// Extend Express types
declare global {
  namespace Express {
    interface Request {
      user?: User;
    }
  }
}

// Type-safe request handlers
interface TypedRequest<
  TBody = unknown,
  TQuery = unknown,
  TParams = unknown,
> extends Request {
  body: TBody;
  query: TQuery;
  params: TParams;
}

interface TypedResponse<TData = unknown> extends Response {
  json: (data: TData) => this;
}

// Validation middleware factory
function validate<T>(schema: z.ZodSchema<T>) {
  return (req: Request, res: Response, next: NextFunction) => {
    try {
      req.body = schema.parse(req.body);
      next();
    } catch (error) {
      if (error instanceof z.ZodError) {
        res.status(400).json({ errors: error.errors });
      } else {
        next(error);
      }
    }
  };
}

// Typed route handlers
type RouteHandler<
  TBody = unknown,
  TQuery = unknown,
  TParams = unknown,
  TData = unknown,
> = (
  req: TypedRequest<TBody, TQuery, TParams>,
  res: TypedResponse<TData>,
  next: NextFunction,
) => void | Promise<void>;

// Example usage
const CreateUserSchema = z.object({
  email: z.string().email(),
  name: z.string(),
  age: z.number().optional(),
});

type CreateUserBody = z.infer<typeof CreateUserSchema>;
type CreateUserResponse = { user: User };

const createUserHandler: RouteHandler<
  CreateUserBody,
  {},
  {},
  CreateUserResponse
> = async (req, res) => {
  const user = await db.createUser(req.body);
  res.json({ user });
};

const app = express();
app.post("/users", validate(CreateUserSchema), createUserHandler);

// Error handling with discriminated unions
type ApiError =
  | { type: "validation"; errors: z.ZodError }
  | { type: "not_found"; resource: string }
  | { type: "unauthorized"; message: string }
  | { type: "internal"; error: Error };

class AppError extends Error {
  constructor(public readonly error: ApiError) {
    super(error.type);
  }
}

function errorHandler(
  err: Error,
  req: Request,
  res: Response,
  next: NextFunction,
) {
  if (err instanceof AppError) {
    switch (err.error.type) {
      case "validation":
        return res.status(400).json({ errors: err.error.errors.errors });
      case "not_found":
        return res
          .status(404)
          .json({ message: `${err.error.resource} not found` });
      case "unauthorized":
        return res.status(401).json({ message: err.error.message });
      case "internal":
        return res.status(500).json({ message: "Internal server error" });
    }
  }
  res.status(500).json({ message: "Unknown error" });
}

app.use(errorHandler);
```

### Async Patterns and Error Handling

```typescript
// Result type for error handling without exceptions
type Result<T, E = Error> = { ok: true; value: T } | { ok: false; error: E };

async function fetchUser(id: string): Promise<Result<User>> {
  try {
    const response = await fetch(`/api/users/${id}`);
    if (!response.ok) {
      return { ok: false, error: new Error(`HTTP ${response.status}`) };
    }
    const user = await response.json();
    return { ok: true, value: user };
  } catch (error) {
    // `error` is `unknown` (useUnknownInCatchVariables, on via strict since 4.4);
    // normalize instead of the unsafe `error as Error`, which can yield an object
    // whose `.message` is undefined when a non-Error is thrown.
    const err = error instanceof Error ? error : new Error(String(error));
    return { ok: false, error: err };
  }
}

// Usage
const result = await fetchUser("123");
if (result.ok) {
  console.log(result.value.name);
} else {
  console.error(result.error.message);
}

// Type-safe Promise utilities
async function race<T extends readonly unknown[]>(promises: {
  [K in keyof T]: Promise<T[K]>;
}): Promise<T[number]> {
  return Promise.race(promises);
}

async function all<T extends readonly unknown[]>(promises: {
  [K in keyof T]: Promise<T[K]>;
}): Promise<T> {
  return Promise.all(promises) as Promise<T>;
}

// Usage with type inference
const [user, posts, comments] = await all([
  fetchUser("123"),
  fetchPosts("123"),
  fetchComments("123"),
]);
// Each element is correctly typed

// Retry with exponential backoff
async function retry<T>(
  fn: () => Promise<T>,
  options: {
    maxAttempts: number;
    initialDelay: number;
    maxDelay: number;
    backoffFactor: number;
  },
): Promise<T> {
  // `unknown` + a sentinel: avoids both the unsafe `error as Error` cast and the
  // uninitialized-variable hazard if the loop never runs (e.g. maxAttempts <= 0).
  let lastError: unknown = new Error("retry: no attempts made");
  let delay = options.initialDelay;

  for (let attempt = 0; attempt < options.maxAttempts; attempt++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error instanceof Error ? error : new Error(String(error));
      if (attempt < options.maxAttempts - 1) {
        await new Promise((resolve) => setTimeout(resolve, delay));
        delay = Math.min(delay * options.backoffFactor, options.maxDelay);
      }
    }
  }

  throw lastError;
}

// Usage
const user = await retry(() => fetchUser("123"), {
  maxAttempts: 3,
  initialDelay: 1000,
  maxDelay: 10000,
  backoffFactor: 2,
});
```

## Anti-Patterns

### Avoid These Practices

```typescript
// BAD: Using `any` to bypass type checking
function process(data: any): any {
  return data.foo.bar.baz;
}

// GOOD: Use unknown and narrow the type
function process(data: unknown): string {
  if (isValidData(data)) {
    return data.foo.bar.baz;
  }
  throw new Error("Invalid data");
}

// BAD: Type assertions without validation
const user = JSON.parse(input) as User;

// GOOD: Validate at runtime (use zod, io-ts, etc.)
import { z } from "zod";

const UserSchema = z.object({
  id: z.string(),
  email: z.string().email(),
  name: z.string(),
});

const user = UserSchema.parse(JSON.parse(input));

// BAD: Non-null assertion operator abuse
function getUser(id: string): User {
  return users.find((u) => u.id === id)!; // Crashes if not found
}

// GOOD: Handle the undefined case
function getUser(id: string): User | undefined {
  return users.find((u) => u.id === id);
}

// Or throw explicitly
function getUser(id: string): User {
  const user = users.find((u) => u.id === id);
  if (!user) {
    throw new Error(`User not found: ${id}`);
  }
  return user;
}

// BAD: Overly permissive function signatures
function merge(a: object, b: object): object {
  return { ...a, ...b };
}

// GOOD: Use generics to preserve types
function merge<T extends object, U extends object>(a: T, b: U): T & U {
  return { ...a, ...b };
}

// BAD: numeric enums accept any number (let s: Status = 999 compiles — a
// type-safety hole); enums also error under --erasableSyntaxOnly and cannot be
// stripped by Node.js native TypeScript (Node 22.18+).
enum Status {
  Pending,
  Active,
  Completed,
}

function activate(s: Status) {}
activate(999); // No error — any number is assignable to a numeric enum

// GOOD: Use const objects or union types — zero runtime cost, no assignment
// hole, strips cleanly.
const Status = {
  Pending: "pending",
  Active: "active",
  Completed: "completed",
} as const;

type Status = (typeof Status)[keyof typeof Status];

// BAD: Interface merging by accident
interface Config {
  port: number;
}

interface Config {
  host: string;
}
// Now Config has both port and host - often unintentional

// GOOD: Use type aliases when you don't want merging
type Config = {
  port: number;
  host: string;
};

// BAD: Ignoring strictNullChecks issues
function getLength(str: string | null): number {
  return str.length; // Runtime error if null
}

// GOOD: Proper null handling
function getLength(str: string | null): number {
  return str?.length ?? 0;
}
```

### Quick Pattern Swaps

```typescript
// BAD: async callbacks inside forEach
async function saveAll(items: Item[]) {
  items.forEach(async (item) => {
    await save(item);
  });
}

// GOOD: Use Promise.all or a for...of loop
async function saveAll(items: Item[]) {
  await Promise.all(items.map((item) => save(item)));
}

// BAD: Boolean flags that allow impossible states
type RequestState = {
  isLoading: boolean;
  data?: User[];
  error?: Error;
};

// GOOD: Use a discriminated union for each valid state
type RequestState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "success"; data: User[] }
  | { status: "error"; error: Error };
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

### Idioms

#### `satisfies` — validate without widening

`satisfies` (TS 4.9+) checks that an expression is assignable to a target type **without** replacing the expression's inferred type with that target, so downstream code keeps the narrowest per-property/literal types while still catching wrong shapes and typo'd keys. It resolves the dilemma between a `: Type` annotation (validates but WIDENS, losing literal/tuple precision) and an `as Type` assertion (preserves nothing and SUPPRESSES mismatches, so a misspelled key slips through). Mechanism: TypeScript verifies assignability to the target but records the original expression type for inference. Constraint: it applies only at an expression/initializer site — it is not a statement you can retroactively apply to an already-declared variable.

```typescript
type Colors = "red" | "green" | "blue";
type RGB = [number, number, number];

const palette = {
  red: [255, 0, 0],
  green: "#00ff00",
  // bleu: [0, 0, 255]  // ← Error: 'bleu' is not in Record<Colors, ...>
} satisfies Record<Colors, string | RGB>;

palette.green.toUpperCase(); // OK — still narrowed to string
palette.red.at(0); // OK — still narrowed to [number, number, number]

// An `as` assertion would suppress everything — a typo'd key compiles silently:
// const p = { red: [255,0,0], bleu: [0,0,255] } as Record<Colors, string | RGB>;
```

#### `NoInfer<T>` — mark a parameter validate-only

`NoInfer<T>` (TS 5.4+) tells TypeScript not to use a parameter as an inference candidate for a type variable, while still validating it against the `T` inferred from the principal parameters. Without it, every `T`-typed parameter contributes inference candidates, so a default value or callback can silently expand what `T` resolves to and accept out-of-range values. Use it when one parameter is the authoritative source of truth.

```typescript
function createStreetLight<C extends string>(
  colors: C[],
  defaultColor?: NoInfer<C>, // validated against C, never widens it
) {}

createStreetLight(["red", "yellow", "green"], "blue");
// Error: '"blue"' is not assignable to '"red" | "yellow" | "green" | undefined'
// Without NoInfer, 'blue' would be absorbed into C and accepted.
```

#### `const` type parameters — keep the `readonly` constraint

The `const` modifier on a type parameter (TS 5.0+) makes inline literal arguments infer const-like (literal/tuple) types, so callers no longer need `as const`. The silent trap: if the constraint is **mutable** (`T extends string[]`), the const-inferred candidate `readonly ['a','b']` is not assignable to it, so inference falls back to the widened mutable type with NO warning. Always use a `readonly` constraint. The modifier also affects only literals written directly at the call site — passing a pre-declared variable (already inferred as `string[]`) sees no benefit.

```typescript
declare function tags<const T extends readonly string[]>(args: T): T;
const t = tags(["a", "b"]); // readonly ['a', 'b']

// With `T extends string[]` (mutable), tags(['a','b']) silently widens to string[].
```

#### `verbatimModuleSyntax` and precise type-only imports

`verbatimModuleSyntax` (TS 5.0+) replaces the deprecated, now-no-op `importsNotUsedAsValues`/`preserveValueImports` with one rule: imports/exports WITHOUT a `type` modifier are emitted verbatim; anything WITH `type` is erased. So every purely-type import must be `import type { ... }` or use an inline `type` specifier — otherwise it is emitted as a runtime import even when unused, defeating tree-shaking, forcing unwanted CJS/ESM `require()` inclusion, and breaking cross-compiler consistency (esbuild/swc/Babel all strip `type`-marked imports reliably). It is in TS 5.9's `tsc --init` defaults. (See Module Organization above.)

#### `using` / `await using` — deterministic cleanup (TS 5.2)

Explicit Resource Management: any object implementing `Symbol.dispose` can be declared with `using`, and TypeScript guarantees `dispose` runs on scope exit — including early returns and exceptions — in last-in-first-out order. `await using` calls and awaits `Symbol.asyncDispose`. This replaces error-prone `try/finally` cleanup for DB connections, file handles, timers, and test fixtures. Requires `lib` to include `esnext.disposable`; some runtimes need a `Symbol.dispose` polyfill.

```typescript
class DbConnection implements Disposable {
  constructor(private conn: Connection) {}
  [Symbol.dispose]() {
    this.conn.close();
  }
}
async function processRecords() {
  using db = new DbConnection(openConnection());
  return await db.conn.query("SELECT * FROM records"); // close() runs on every exit path
}
// tsconfig: { "lib": ["es2022", "esnext.disposable"] }
```

### Anti-Patterns

#### Explicit `x is T` predicates are trusted unconditionally — as unsafe as `as`

When you annotate a guard's return type as `x is T`, TypeScript does NOT verify the body actually narrows `x` to `T` — it trusts the assertion, making an explicit predicate semantically equivalent to a type assertion. A wrong or incomplete predicate compiles silently and causes runtime type confusion. Prefer letting TypeScript INFER the predicate from a simple narrowing body (TS 5.5+), because then the compiler derives it from the implementation. Reserve explicit `is` for cases inference cannot handle (multiple return paths, deep structural validation) — write them thoroughly and unit-test them.

```typescript
// Inferred & validated by the compiler:
const isString = (x: unknown) => typeof x === "string"; // inferred: x is string

// Compiles fine but is a lie — TypeScript never checks the body:
function isPositive(n: number): n is 1 | 2 | 3 {
  return n > 0; // also true for 4, 5, … → downstream runtime crash
}
```

#### Never publish `const enum` in a `.d.ts`

A published `const enum` is inlined into consumers' bundles at compile time. If a later patch changes member values, consumers keep the OLD inlined values while running the NEW library — a silent wrong-branch bug. It is also incompatible with `isolatedModules` and single-file transpilers (Babel, esbuild, swc), which cannot inline cross-file values. For published APIs use a regular `enum`, an `as const` object, or `preserveConstEnums` to strip the `const` from declaration output.

```typescript
// Safe in a published package — value exists at runtime, no inlining:
const Direction = { Up: "UP", Down: "DOWN" } as const;
type Direction = (typeof Direction)[keyof typeof Direction];
```

### Gotchas

#### TS 5.5 inferred type predicates — but truthiness and `.filter(Boolean)` do NOT narrow

TS 5.5 infers a type predicate for a function with no explicit return annotation, a single return statement, no parameter mutation, and a boolean expression tied to a refinement of the parameter — so `arr.filter(x => x !== undefined)` finally returns `T[]`. The trap: truthiness checks (`x => !!x`, `x => x`) and `.filter(Boolean)` do NOT infer a predicate. Reason — the "if and only if" rule: `!!score` being false could mean `undefined` OR the valid value `0`, so `score is number` would be unsound; `Boolean` is also not itself recognized as a predicate. The result is doubly bad: the type stays `(T | undefined)[]` AND zero/empty-string values are silently dropped at runtime. Use explicit comparisons or a named guard.

```typescript
// Inferred predicate → Bird[]
const birds = countries.map((c) => birdMap.get(c)).filter((b) => b !== undefined);

// Reusable named guard for .filter(Boolean) situations:
function isDefined<T>(x: T | null | undefined): x is NonNullable<T> {
  return x != null;
}
const defined = countries.map((c) => birdMap.get(c)).filter(isDefined); // Bird[]

// BAD: type stays (number | undefined)[] AND zero scores are dropped:
// students.map(s => scoreMap.get(s)).filter(score => !!score);
```

#### `useUnknownInCatchVariables` types `catch` as `unknown` (on via `strict` since 4.4)

Catch-clause variables are `unknown`, not `any`, so touching `.message`/`.stack` without a guard fails to compile — and the break rides in silently via `strict` on upgrade. Use an `instanceof Error` guard; `error as Error` restores the old unsafe behavior and is only a temporary migration crutch (see the `fetchUser`/`retry` examples above).

```typescript
try {
  await riskyOperation();
} catch (err) {
  if (err instanceof Error) console.error(err.message);
  else console.error("Unknown error:", String(err));
}
```

#### Excess-property checking only fires on FRESH object literals

The "may only specify known properties" error fires only when a literal is assigned DIRECTLY to a typed target or passed DIRECTLY as an argument. Assigning the same literal to an intermediate variable first — even one with an explicit type annotation — strips its freshness, and structural typing then allows the extra properties silently. Refactoring a direct literal into a named variable "for readability" can suppress a real bug the compiler was catching.

```typescript
interface Duck {
  quack(): void;
}
const d: Duck = { quack() {}, woof() {} }; // Error: 'woof' is excess on a fresh literal

const obj = { quack() {}, woof() {} }; // freshness lost
const d2: Duck = obj; // No error — extra 'woof' silently allowed
```

#### Control-flow narrowing is discarded inside closures — copy to a `const`

TypeScript drops a variable's narrowing when it is captured by a closure, even if unconditionally assigned beforehand, because the captured binding could be reassigned between narrowing and execution (acknowledged design limitation, microsoft/TypeScript#37802). Copy the narrowed value into a fresh `const` so the closure captures an immutable binding.

```typescript
function deferred(value?: string): () => string {
  if (value == null) value = "";
  const v = value; // const captures the narrowed type
  return () => v; // v is string — returning () => value would widen to string | undefined
}
```

#### Method-shorthand syntax is checked bivariantly — `strictFunctionTypes` does not catch it

`strictFunctionTypes` enforces contravariant parameter checking for function-TYPED properties (`m: (x: T) => void`), but the docs explicitly exempt parameters of methods declared in shorthand syntax (`m(x: T): void`) — these stay BIVARIANT. The exemption lets `Array<T>` relate covariantly, but in user-defined interfaces it is a real soundness hole. The typescript-eslint rule `method-signature-style` can force property syntax to close the gap.

```typescript
interface Processor {
  process: (value: string | number) => void; // property syntax → contravariant
}
const p: Processor = {
  process: (value: string) => console.log(value), // Error: string not assignable to string | number
};
// With method shorthand `process(value: string | number): void`, the same
// assignment compiles — and crashes at runtime if called with a number.
```

#### Template-literal types produce Cartesian products

A template-literal type interpolating multiple unions expands to the full Cartesian product: unions of size N and M yield N*M members, growing multiplicatively and becoming a real compile-time cost for large schemas. The bounded event-name pattern is the canonical good use; for large route maps or i18n keys prefer code generation (`tsc --generateTrace` surfaces the cost). Note the intrinsic `Uppercase`/`Lowercase`/`Capitalize` types use raw JS `toUpperCase`/`toLowerCase` — they are NOT locale-aware.

```typescript
type PropEventSource<T> = {
  on<K extends string & keyof T>(event: `${K}Changed`, cb: (v: T[K]) => void): void;
};
// AVOID: `${Methods} ${Routes}` over 5 × 50 unions → 250 members; prefer codegen.
```

### Performance

#### Prefer `interface extends` over type intersection for object types

`interface Foo extends Bar, Baz` produces a single flat object type whose relationships the compiler caches, whereas `type Foo = Bar & Baz` forces a recursive merge of constituents on every comparison at each use site. Effects: faster type-checking / better language-server responsiveness in large codebases; conflicting properties are reported eagerly at the declaration instead of silently collapsing to `never` at use sites; cleaner IDE hover. The TS Performance wiki names this a high-impact optimization. Intersections remain necessary for composing non-object types (unions, primitives, mapped/conditional results).

```typescript
interface AdminUser extends BaseUser, AdminPermissions {
  adminLevel: number;
}
// type AdminUser = BaseUser & AdminPermissions & { adminLevel: number }; ← recomputed per comparison
```

#### `isolatedDeclarations` unlocks parallel `.d.ts` emit

`isolatedDeclarations` (TS 5.5+) requires explicit type annotations on all exported symbols so each file's `.d.ts` can be generated independently, without a whole-program type-checker pass — letting tools (Oxc, esbuild, swc) emit declarations in parallel and removing the monorepo serialization bottleneck. Requires `declaration` or `composite`. Tradeoff: explicit return types on exported functions become mandatory; it pays off most when you already enforce explicit-return-types via ESLint.

```typescript
// isolatedDeclarations: true (with declaration: true)
export function computeTotal(items: Item[]): number {
  return items.reduce((sum, i) => sum + i.price, 0);
}
```

### Currency

#### Import attributes: `with { type: 'json' }`, not `assert`

Import assertions using the withdrawn `assert` keyword were superseded by import attributes using `with` (ES2025). Under `--module nodenext`, TS 5.8 makes `assert` a hard error (matching Node.js 22+), and TS 5.7 already required `with` for validated JSON imports under nodenext. Migrate all `assert { type: 'json' }` to `with { type: 'json' }`.

```typescript
import config from "./config.json" with { type: "json" };
```

#### TS 6.0 deprecates / TS 7.0 (Go compiler) removes `es5` target, `baseUrl`, node10 resolution

TS 6.0 is the LAST JavaScript-based release; it DEPRECATES, and the Go-rewritten TS 7.0 REMOVES: `--target es3`/`es5` (ES2015 becomes the minimum), `--baseUrl` (migrate path aliases to the Node-native `package.json#imports` map, supported by both Node and TS without a build step), and `moduleResolution: node10`/classic. Down-leveling for ancient targets belongs in Babel/esbuild, not `tsc`. The deprecations land in 6.0; the removals in 7.0 — do not conflate the two.

```json
{ "imports": { "#utils/*": "./src/utils/*.js", "#models/*": "./src/models/*.js" } }
```

