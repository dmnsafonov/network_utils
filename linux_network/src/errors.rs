error_chain!(
    errors {
        IfNameTooLong(name: String) {
            description("interface name too long")
            display("interface name \"{}\" is too long", name)
        }
    }

    foreign_links {
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
