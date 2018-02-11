#[macro_use] extern crate error_chain;

use ::std::any::Any;
use ::std::ops::*;

pub mod errors {
    error_chain!(
        errors {
            InvalidArgument(descr: String) {
                description("invalid argument")
                display("invalid argument: {}", descr)
            }
        }
    );
}

use self::errors::*;

// the complexity is due to #[repr(TYPE_ALIAS)] still being unimplemented
// TODO: rewrite with #[repr(TYPE_ALIAS)]

pub trait EnumToNum<T> where T: BitAnd + BitOr + BitOrAssign + Copy {
    fn to_num(&self) -> T;
}

pub trait EnumFromNum<T> where
        T: BitAnd + BitOr + BitOrAssign + Copy,
        Self: Sized {
    fn from_num(x: T) -> Result<Self>;
}

pub trait EnumFromNumArg<T> where
        T: BitAnd + BitOr + BitOrAssign + Copy,
        Self: Sized {
    fn from_num(x: T, arg: &Any) -> Result<Self>;
}

pub trait EnumFromNumArgRef<'a, T> where
        T: BitAnd + BitOr + BitOrAssign + Copy,
        Self: Sized {
    fn from_num(x: T, arg: &'a Any) -> Result<Self>;
}

#[macro_export]
macro_rules! gen_enum {
    ( $name:ident: $tp:ty; $( $t:tt )+ ) => (
        _gen_enum!( $name; $( $t )* );
        _gen_conversions!( $name; $tp; $( $t )* );
    );
    ( pub $name:ident: $tp:ty; $( $t:tt )+ ) => (
        _gen_enum!( pub $name; $( $t )* );
        _gen_conversions!( $name; $tp; $( $t )* );
    )
}

#[macro_export]
macro_rules! gen_enum_arg {
    ( $name:ident: $tp:ty; $( $t:tt )+ ) => (
        _gen_enum_arg!( $name; (); $( $t )*, );
        _gen_conversions_arg!( $name; $tp; $( $t )* );
    );
    ( pub $name:ident: $tp:ty; $( $t:tt )+ ) => (
        _gen_enum_arg!( pub $name; (); $( $t )*, );
        _gen_conversions_arg!( $name; $tp; $( $t )* );
    )
}

#[macro_export]
macro_rules! gen_enum_arg_ref {
    ( $name:ident: $tp:ty; $( $t:tt )+ ) => (
        _gen_enum_arg_ref!( $name; (); $( $t )*, );
        _gen_conversions_arg_ref!( $name; $tp; $( $t )* );
    );
    ( pub $name:ident: $tp:ty; $( $t:tt )+ ) => (
        _gen_enum_arg_ref!( pub $name; (); $( $t )*, );
        _gen_conversions_arg_ref!( $name; $tp; $( $t )* );
    )
}

#[macro_export]
macro_rules! _gen_enum {
    ( $name:ident; $( ( $cnst:ident => $var:ident ) ),+ ) => (
        #[allow(dead_code)]
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        enum $name {
            $( $var ),*
        }
    );
    ( pub $name:ident; $( ( $cnst:ident => $var:ident ) ),+ ) => (
        #[allow(dead_code)]
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum $name {
            $( $var ),*
        }
    )
}

#[macro_export]
macro_rules! _gen_enum_arg {
    ( $name:ident; ( $( $acc:tt )* ); ( $cnst:ident => $var:ident($t:ty) ), $( $o:tt )* ) => (
        _gen_enum_arg!($name; ( $( $acc )* $var($t), ); $( $o )* );
    );
    ( $name:ident; ( $( $acc:tt )* ); ( $cnst:ident => $var:ident ), $( $o:tt )* ) => (
        _gen_enum_arg!($name; ( $( $acc )* $var, ); $( $o )* );
    );
    ( pub $name:ident; ( $( $acc:tt )* ); ( $cnst:ident => $var:ident($t:ty) ), $( $o:tt )* ) => (
        _gen_enum_arg!(pub $name; ( $( $acc )* $var($t), ); $( $o )* );
    );
    ( pub $name:ident; ( $( $acc:tt )* ); ( $cnst:ident => $var:ident ), $( $o:tt )* ) => (
        _gen_enum_arg!(pub $name; ( $( $acc )* $var, ); $( $o )* );
    );

    ( $name:ident; ( $(,)* $( $acc:tt )+ ); $( $o:tt )* ) => (
        #[allow(dead_code)]
        #[derive(Clone, Debug)]
        enum $name {
            $( $acc )*
        }
    );
    ( pub $name:ident; ( $(,)* $( $acc:tt )+ ); $( $o:tt )* ) => (
        #[allow(dead_code)]
        #[derive(Clone, Debug)]
        pub enum $name {
            $( $acc )*
        }
    )
}

#[macro_export]
macro_rules! _gen_enum_arg_ref {
    ( $name:ident; ( $( $acc:tt )* ); ( $cnst:ident => $var:ident(ref $t:ty) ), $( $o:tt )* ) => (
        _gen_enum_arg_ref!($name; ( $( $acc )* $var(&'_b $t), ); $( $o )* );
    );
    ( $name:ident; ( $( $acc:tt )* ); ( $cnst:ident => $var:ident($t:ty) ), $( $o:tt )* ) => (
        _gen_enum_arg_ref!($name; ( $( $acc )* $var($t), ); $( $o )* );
    );
    ( $name:ident; ( $( $acc:tt )* ); ( $cnst:ident => $var:ident ), $( $o:tt )* ) => (
        _gen_enum_arg_ref!($name; ( $( $acc )* $var, ); $( $o )* );
    );
    ( pub $name:ident; ( $( $acc:tt )* ); ( $cnst:ident => $var:ident(ref $t:ty) ), $( $o:tt )* ) => (
        _gen_enum_arg_ref!(pub $name; ( $( $acc )* $var(&'_b $t), ); $( $o )* );
    );
    ( pub $name:ident; ( $( $acc:tt )* ); ( $cnst:ident => $var:ident($t:ty) ), $( $o:tt )* ) => (
        _gen_enum_arg_ref!(pub $name; ( $( $acc )* $var($t), ); $( $o )* );
    );
    ( pub $name:ident; ( $( $acc:tt )* ); ( $cnst:ident => $var:ident ), $( $o:tt )* ) => (
        _gen_enum_arg_ref!(pub $name; ( $( $acc )* $var, ); $( $o )* );
    );

    ( $name:ident; ( $(,)* $( $acc:tt )+ ); $( $o:tt )* ) => (
        #[allow(dead_code)]
        #[derive(Debug)]
        enum $name<'_b> {
            $( $acc )*
        }
    );
    ( pub $name:ident; ( $(,)* $( $acc:tt )+ ); $( $o:tt )* ) => (
        #[allow(dead_code)]
        #[derive(Debug)]
        pub enum $name<'_b> {
            $( $acc )*
        }
    )
}

#[macro_export]
macro_rules! _gen_conversions {
    ( $($t:tt)+ ) => (
        _gen_to_conv!( $($t)* );
        _gen_from_conv!( $($t)* );
    )
}

#[macro_export]
macro_rules! _gen_conversions_arg {
    ( $($t:tt)+ ) => (
        _gen_to_conv!( $($t)* );
        _gen_from_conv_arg!( $($t)* );
    )
}

#[macro_export]
macro_rules! _gen_conversions_arg_ref {
    ( $($t:tt)+ ) => (
        _gen_to_conv_ref!( $($t)* );
        _gen_from_conv_arg_ref!( $($t)* );
    )
}

#[macro_export]
macro_rules! _gen_to_conv {
    ( $name:ident; $tp:ty; $( ($cnst:ident => $( $t:tt )+ ) ),+ ) => (
        #[allow(dead_code, unreachable_patterns)]
        impl $crate::EnumToNum<$tp> for $name {
            fn to_num(&self) -> $tp {
                match *self {
                    $( _gen_to_conv_match!( $name; $( $t )* ) => $cnst as $tp ),*
                }
            }
        }
    )
}

#[macro_export]
macro_rules! _gen_to_conv_ref {
    ( $name:ident; $tp:ty; $( ($cnst:ident => $( $t:tt )+ ) ),+ ) => (
        #[allow(dead_code, unreachable_patterns)]
        impl<'_a> $crate::EnumToNum<$tp> for $name<'_a> {
            fn to_num(&self) -> $tp {
                match *self {
                    $( _gen_to_conv_match!( $name; $( $t )* ) => $cnst as $tp ),*
                }
            }
        }
    )
}

#[macro_export]
macro_rules! _gen_to_conv_match {
    ( $name:ident; $var:ident( $( $t:tt )+ ) ) => ( $name::$var(_) );
    ( $name:ident; $var:ident ) => ( $name::$var )
}

#[macro_export]
macro_rules! _gen_from_conv {
    ( $name:ident; $tp:ty; $( ( $cnst:ident => $var:ident ) ),+ ) => (
        #[allow(dead_code, unreachable_patterns)]
        impl $crate::EnumFromNum<$tp> for $name {
            fn from_num(x: $tp) -> $crate::errors::Result<$name> {
                Ok(match x {
                    $( $cnst => $name::$var ),* ,
                    _ => {
                        let k = $crate::errors::ErrorKind::InvalidArgument(
                            format!("{} is not a valid {} enum variant",
                                x, stringify!($name))
                        );
                        return Err(k.into())
                    }
                })
            }
        }
    )
}

#[macro_export]
macro_rules! _gen_from_conv_arg {
    ( $name:ident; $tp:ty; $( ( $cnst:ident => $( $t:tt )+ ) ),+ ) => (
        #[allow(dead_code, unreachable_patterns)]
        impl $crate::EnumFromNumArg<$tp> for $name {
            fn from_num(x: $tp, opt: &::std::any::Any)
                    -> $crate::errors::Result<$name> {
                Ok(match x {
                    $( $cnst => _gen_conv_from_arg_match!($name; opt; $( $t )* ) ),* ,
                    _ => {
                        let k = $crate::errors::ErrorKind::InvalidArgument(
                            format!("{} is not a valid {} enum variant",
                                x, stringify!($name))
                        );
                        return Err(k.into())
                    }
                })
            }
        }
    )
}

#[macro_export]
macro_rules! _gen_from_conv_arg_ref {
    ( $name:ident; $tp:ty; $( ( $cnst:ident => $( $t:tt )+ ) ),+ ) => (
        #[allow(dead_code, unreachable_patterns)]
        impl<'_a> $crate::EnumFromNumArgRef<'_a, $tp> for $name<'_a> {
            fn from_num(x: $tp, opt: &'_a ::std::any::Any)
                    -> $crate::errors::Result<$name<'_a>> {
                Ok(match x {
                    $( $cnst => _gen_conv_from_arg_match!($name; opt; $( $t )* ) ),* ,
                    _ => return Err($crate::errors::ErrorKind::InvalidArgument(
                        format!("{} is not a valid {} enum variant",
                            x, stringify!($name)))
                        .into())
                })
            }
        }
    )
}

#[macro_export]
macro_rules! _gen_conv_from_arg_match {
    ( $name:ident; $opt:ident; $var:ident(ref $t:ty) ) => (
        $name::$var($opt.downcast_ref::<&$t>().unwrap())
    );
    ( $name:ident; $opt:ident; $var:ident($t:ty) ) => (
        $name::$var($opt.downcast_ref::<$t>().unwrap().clone())
    );
    ( $name:ident; $opt:ident; $var:ident ) => (
        $name::$var
    )
}

pub trait NumEnumFlagSet<F, T> where
        F: EnumToNum<T> + Copy,
        T: BitAnd + BitOr + BitOrAssign + Copy,
        Self: Copy {
    fn new() -> Self;
    unsafe fn from_num(x: T) -> Self;
    fn set(&self, flag: F) -> Self;
    fn clear(&self, flag: F) -> Self;
    fn test(&self, flag: F) -> bool;
    fn get(&self) -> T;
}

#[macro_export]
macro_rules! gen_flag_set {
    ( $name:ident, $t:ty: $it:ty ) => (
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        struct $name($it);

        _gen_flag_set_impl!($name, $t, $it);
    );
    ( pub $name:ident, $t:ty: $it:ty ) => (
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub struct $name($it);

        _gen_flag_set_impl!($name, $t, $it);
    )
}

#[macro_export]
macro_rules! _gen_flag_set_impl {
    ( $name:ident, $t:ty, $it:ty ) => (
        impl $crate::NumEnumFlagSet<$t, $it> for $name {
            fn new() -> $name {
                $name(0)
            }

            unsafe fn from_num(x: $it) -> $name {
                $name(x)
            }

            fn set(&self, flag: $t) -> $name {
                use $crate::EnumToNum;
                $name(self.0 | flag.to_num())
            }

            fn clear(&self, flag: $t) -> $name {
                use $crate::EnumToNum;
                $name(self.0 & !flag.to_num())
            }

            fn test(&self, flag: $t) -> bool {
                use $crate::EnumToNum;
                (self.0 & flag.to_num()) != 0
            }

            fn get(&self) -> $it {
                self.0
            }
        }

        impl ::std::ops::BitOr for $name {
            type Output = $name;

            fn bitor(self, rhs: $name) -> Self::Output {
                $name((self.0 | rhs.0))
            }
        }

        impl ::std::ops::BitOr<$t> for $name {
            type Output = $name;

            fn bitor(self, rhs: $t) -> Self::Output {
                use $crate::EnumToNum;
                $name(self.0 | rhs.to_num())
            }
        }

        impl ::std::ops::BitOrAssign for $name {
            fn bitor_assign(&mut self, rhs: $name) {
                self.0 |= rhs.0;
            }
        }

        impl ::std::ops::BitOrAssign<$t> for $name {
            fn bitor_assign(&mut self, rhs: $t) {
                use $crate::EnumToNum;
                self.0 |= rhs.to_num()
            }
        }

        impl ::std::ops::BitOr for $t {
            type Output = $name;

            fn bitor(self, rhs: $t) -> Self::Output {
                use $crate::EnumToNum;
                $name(self.to_num() | rhs.to_num())
            }
        }

        impl ::std::ops::BitAnd for $name {
            type Output = $name;

            fn bitand(self, rhs: $name) -> Self::Output {
                $name(self.0 & rhs.0)
            }
        }

        impl ::std::ops::BitAnd<$t> for $name {
            type Output = $name;

            fn bitand(self, rhs: $t) -> Self::Output {
                use $crate::EnumToNum;
                $name(self.0 & rhs.to_num())
            }
        }

        impl ::std::convert::From<$t> for $name {
            fn from(x: $t) -> Self {
                use $crate::NumEnumFlagSet;
                $name::new().set(x)
            }
        }
    )
}
