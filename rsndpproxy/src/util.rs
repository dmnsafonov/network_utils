pub fn log_if_err<T,E>(x: ::std::result::Result<T,E>)
        where E: ::error_chain::ChainedError {
    if let Err(e) = x {
        error!("{}", e.display_chain());
    }
}
