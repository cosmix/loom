from typing import Generic, TypeVar

T = TypeVar("T")


class Boxed(Generic[T]):
    def __init__(self, value: T) -> None:
        self.value = value


def outer(value: T) -> T:
    def inner(item: T) -> T:
        return item

    return inner(value)
