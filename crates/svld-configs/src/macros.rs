#[macro_export]
macro_rules! from_str_impl {
    ($t:ty) => {
        #[inline(always)]
        pub fn from_str(s: &str) -> Result<$t, toml::de::Error> {
            toml::from_str::<$t>(s)
        }
    };
}
