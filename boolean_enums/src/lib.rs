#[macro_export]
macro_rules! gen_boolean_enum {
    ($name:ident) => (
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum $name {
            Yes,
            No
        }

        impl From<bool> for $name {
            fn from(x: bool) -> $name {
                match x {
                    true => $name::Yes,
                    false => $name::No
                }
            }
        }

        impl Into<bool> for $name {
            fn into(self) -> bool {
                match self {
                    $name::Yes => true,
                    $name::No => false
                }
            }
        }
    );

    (pub $name:ident) => (
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name {
            Yes,
            No
        }

        impl From<bool> for $name {
            fn from(x: bool) -> $name {
                match x {
                    true => $name::Yes,
                    false => $name::No
                }
            }
        }

        impl Into<bool> for $name {
            fn into(self) -> bool {
                match self {
                    $name::Yes => true,
                    $name::No => false
                }
            }
        }
    )
}
