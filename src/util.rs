#[macro_export]
macro_rules! tryfrom_enum {
    {
        $(#[$attr:meta])*
        enum $name:ident(repr($r:ident)) {
            $($variant:tt = $value:literal,)+
        }
    } => {
        $(#[$attr])*
        #[repr($r)]
        enum $name {
            $($variant = $value,)*
        }

        impl TryFrom<$r> for $name {
            type Error = ();
            fn try_from(value: $r) -> Result<Self, Self::Error> {
                match value {
                    $(x if x == $name::$variant as $r => Ok($name::$variant),)*
                    _ => Err(()),
                }
            }
        }
    };
}
