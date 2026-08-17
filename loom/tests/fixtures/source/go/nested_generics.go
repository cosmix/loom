package source

type Boxed[T any] struct {
	Value T
}

func Outer[T any](value T) T {
	inner := func(item T) T {
		return item
	}

	return inner(value)
}
