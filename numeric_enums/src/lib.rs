#![warn(bare_trait_objects)]

// the need for that crate itself arises due to #[repr(TYPE_ALIAS)]
// still being unimplemented
// TODO: replace #[repr(TYPE_ALIAS)]

#[macro_export]
macro_rules! gen_enum {
    ( $name:ident: $tp:ty; $( $t:tt )+ ) => (
        _gen_enum!( $name; $( $t )* );
        _gen_conversions!( $name; $tp; $( $t )* );
    );
    ( pub $name:ident: $tp:ty; $( $t:tt )+ ) => (
        _gen_enum!( pub $name; $( $t )* );
        _gen_conversions!( pub $name; $tp; $( $t )* );
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
macro_rules! _gen_conversions {
    ( $name:ident; $($t:tt)+ ) => (
        impl $name {
            _gen_to_conv!( $name; $($t)* );
            _gen_from_conv!( $name; $($t)* );
        }
    );
    ( pub $name:ident; $($t:tt)+ ) => (
        impl $name {
            _gen_to_conv!( pub $name; $($t)* );
            _gen_from_conv!( pub $name; $($t)* );
        }
    )
}

#[macro_export]
macro_rules! _gen_to_conv {
    ( $name:ident; $tp:ty; $( ($cnst:ident => $( $t:tt )+ ) ),+ ) => (
        #[allow(dead_code, unreachable_patterns)]
        fn bits(&self) -> $tp {
            match *self {
                $( _gen_to_conv_match!( $name; $( $t )* ) => $cnst as $tp ),*
            }
        }
    );
    ( pub $name:ident; $tp:ty; $( ($cnst:ident => $( $t:tt )+ ) ),+ ) => (
        #[allow(dead_code, unreachable_patterns)]
        pub fn bits(&self) -> $tp {
            match *self {
                $( _gen_to_conv_match!( $name; $( $t )* ) => $cnst as $tp ),*
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
        fn from_bits(x: $tp) -> Option<$name> {
            match x {
                $( $cnst => Some($name::$var) ),* ,
                _ => None
            }
        }
    );
    ( pub $name:ident; $tp:ty; $( ( $cnst:ident => $var:ident ) ),+ ) => (
        #[allow(dead_code, unreachable_patterns)]
        pub fn from_bits(x: $tp) -> Option<$name> {
            match x {
                $( $cnst => Some($name::$var) ),* ,
                _ => None
            }
        }
    )
}
