#[macro_export]
macro_rules! LazyLockDefault {
    () => {
        LazyLock::new(|| Default::default())
    };
}
