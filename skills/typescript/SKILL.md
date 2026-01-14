---
name: typescript
description: TypeScript language expertise for writing type-safe, production-quality TypeScript code. Use for TypeScript development, advanced type system features, strict mode, type-safe APIs, and modern frameworks. Triggers: typescript, ts, tsx, type, interface, generic, union, intersection, discriminated union, type guard, type assertion, utility types, conditional types, mapped types, zod, trpc, prisma, react, node, nodejs, deno, bun, npm, pnpm, yarn, type-safe, type safety, tsconfig, strict mode, branded types
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
  message?: string
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

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "lib": ["ES2022"],
    "outDir": "./dist",
    "rootDir": "./src",
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,

    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noImplicitReturns": true,
    "noFallthroughCasesInSwitch": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "exactOptionalPropertyTypes": true,

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
    listener: (payload: T[K]) => void
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
    })
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
interface TypedRequest<TBody = unknown, TQuery = unknown, TParams = unknown>
  extends Request {
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
  TData = unknown
> = (
  req: TypedRequest<TBody, TQuery, TParams>,
  res: TypedResponse<TData>,
  next: NextFunction
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
  next: NextFunction
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
type Result<T, E = Error> =
  | { ok: true; value: T }
  | { ok: false; error: E };

async function fetchUser(id: string): Promise<Result<User>> {
  try {
    const response = await fetch(`/api/users/${id}`);
    if (!response.ok) {
      return { ok: false, error: new Error(`HTTP ${response.status}`) };
    }
    const user = await response.json();
    return { ok: true, value: user };
  } catch (error) {
    return { ok: false, error: error as Error };
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
async function race<T extends readonly unknown[]>(
  promises: { [K in keyof T]: Promise<T[K]> }
): Promise<T[number]> {
  return Promise.race(promises);
}

async function all<T extends readonly unknown[]>(
  promises: { [K in keyof T]: Promise<T[K]> }
): Promise<T> {
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
  }
): Promise<T> {
  let lastError: Error;
  let delay = options.initialDelay;

  for (let attempt = 0; attempt < options.maxAttempts; attempt++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error as Error;
      if (attempt < options.maxAttempts - 1) {
        await new Promise((resolve) => setTimeout(resolve, delay));
        delay = Math.min(delay * options.backoffFactor, options.maxDelay);
      }
    }
  }

  throw lastError!;
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

// BAD: Using enums (they have runtime overhead and quirks)
enum Status {
  Pending,
  Active,
  Completed,
}

// GOOD: Use const objects or union types
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
