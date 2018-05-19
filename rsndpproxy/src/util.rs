pub fn log_if_err<T>(x: ::std::result::Result<T, ::failure::Error>) {
    if let Err(e) = x {
        let mut out = String::new();

        let mut first = true;;
        for i in e.causes() {
            if !first {
                out += ": ";
            }
            out += &format!("{}", i);
            first = false;
        }

        error!("{}", out);
    }
}
