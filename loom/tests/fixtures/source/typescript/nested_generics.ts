export type Boxed<T> = {
  value: T;
};

export function outer<T>(value: T): T {
  function inner<U>(item: U): U {
    return item;
  }

  return inner(value);
}
