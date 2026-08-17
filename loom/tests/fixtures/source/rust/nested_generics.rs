pub struct Boxed<T> {
    pub value: T,
}

pub fn outer<T>(value: T) -> T {
    fn inner<U>(value: U) -> U {
        value
    }

    inner(value)
}
