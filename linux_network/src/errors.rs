error_chain!(
    errors {
        AddrError(addr: String, explanation: String) {
            description("error getting address info")
            display("error getting address {} info: {}", addr, explanation)
        }
        BufferTooSmall(len: ::libc::size_t) {
            description("provided buffer too small")
            display("provided buffer of length {} too small", len)
        }
        IfNameTooLong(name: String) {
            description("interface name too long")
            display("interface name \"{}\" is too long", name)
        }
        NoInterface(name: String) {
            description("cannot find the requested interface")
            display("cannot find interface {}", name)
        }
        WrongSize {
            description("wrone buffer length")
        }
    }

    foreign_links {
        InterfacesError(::interfaces::InterfacesError);
        IoError(::std::io::Error);
        NixError(::nix::Error);
        NullInString(::std::ffi::NulError);
    }
);

pub fn error_to_errno(x: &Error) -> Option<i32> {
    match *x.kind() {
        ErrorKind::IoError(ref e) => e.raw_os_error(),
        _ => None
    }
}
