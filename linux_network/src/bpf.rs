use ::std::fmt::*;

use ::structs::raw::*;

pub struct BpfProg {
    pub filters: Vec<sock_filter>,
    pub fprog: sock_fprog
}

impl BpfProg {
    pub fn get(&self) -> &sock_fprog {
        &self.fprog
    }
}

impl Debug for BpfProg {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{:?}", self.filters)
    }
}

#[macro_export]
macro_rules! bpf_stmt {
    ( $code:expr, $k:expr ) => (
        $crate::structs::raw::sock_filter {
            code: $code.bits(),
            jt: 0 as u8,
            jf: 0 as u8,
            k: $k as u32
        }
    )
}

#[macro_export]
macro_rules! bpf_jump {
    ( $code:expr, $k:expr, $jt:expr, $jf:expr ) => (
        $crate::structs::raw::sock_filter {
            code: $code.bits(),
            jt: $jt as u8,
            jf: $jf as u8,
            k: $k as u32
        }
    )
}

#[macro_export]
macro_rules! bpf_filter {
    ( bpf_stmt!( $( $arg:tt )+ ); $( $o:tt )* ) => (
        bpf_filter!((bpf_stmt!( $( $arg )* ));;; (1);;; $( $o )* )
    );
    ( bpf_jump!( $( $arg:tt )+ ); $( $o:tt )* ) => (
        bpf_filter!((bpf_jump!( $( $arg )* ));;; (1);;; $( $o )* )
    );

    ( ( $( $acc:tt )+ );;; ($len:expr);;; bpf_stmt!( $( $arg:tt )+ ); $( $o:tt )* ) => (
        bpf_filter!( ( $( $acc )*, bpf_stmt!( $( $arg )* ) );;; ($len + 1);;; $( $o )* )
    );
    ( ( $( $acc:tt )+ );;; ($len:expr);;; bpf_jump!( $( $arg:tt )+ ); $( $o:tt )* ) => (
        bpf_filter!( ( $( $acc )*, bpf_jump!( $( $arg )* ) );;; ($len + 1);;; $( $o )* )
    );

    ( ( $( $acc:tt )+ );;; ($len:expr);;; ) => ({
        let mut ret = Box::new(BpfProg {
            filters: vec![ $( $acc )* ],
            fprog: $crate::structs::raw::sock_fprog {
                len: $len,
                filter: ::std::ptr::null_mut()
            }
        });
        ret.fprog.filter = ret.filters.as_mut_ptr();
        ret
    })
}
