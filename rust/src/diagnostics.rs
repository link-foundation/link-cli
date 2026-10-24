//! Best-effort optional diagnostics; storage outcomes remain authoritative.
// Keep formatting lazy at call sites and preserve their stdout/stderr choice.
macro_rules! stderr {
    ($($argument:tt)*) => {{
        use ::std::io::Write as _;
        let _ = ::std::writeln!(::std::io::stderr().lock(), $($argument)*);
    }};
}
macro_rules! stdout {
    ($($argument:tt)*) => {{
        use ::std::io::Write as _;
        let _ = ::std::writeln!(::std::io::stdout().lock(), $($argument)*);
    }};
}
pub(crate) use stderr;
pub(crate) use stdout;
