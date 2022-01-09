pub use alloc::string::{String, ToString};

pub fn map_to_string<X: ToString>(arg: X) -> String {
    arg.to_string()
}

/*
trait LogThru<T, E> {
    fn log_thru<F, O: FnOnce(E) -> F>(self, op: O) -> Result<T, F>;
}

impl<T, E> LogThru<T, E> for Result<T, E> {
    fn log_thru<F, O: FnOnce(E) -> F>(self, op: O) -> Result<T, F> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(op(e)),
        }
    }
}
*/

#[macro_export]
macro_rules! fn_string {
    ($text:expr) => {
        || $text.to_string()
    };
}

#[macro_export]
macro_rules! log_net {
    (error $text:expr) => {error!(
        target: "net",
        "{}",
        $text,
    )};
    (error $fmt:literal, $($arg:expr),+) => {
        error!(target:"net", $fmt, $($arg),+);
    };
    (warn $text:expr) => {warn!(
        target: "net",
        "{}",
        $text,
    )};
    (warn $fmt:literal, $($arg:expr),+) => {
        warn!(target:"net", $fmt, $($arg),+);
    };
    ($text:expr) => {trace!(
        target: "net",
        "{}",
        $text,
    )};
    ($fmt:literal, $($arg:expr),+) => {
        trace!(target:"net", $fmt, $($arg),+);
    }
}

#[macro_export]
macro_rules! log_rpc {
    (error $text:expr) => { error!(
        target: "rpc",
        "{}",
        $text,
    )};
    (error $fmt:literal, $($arg:expr),+) => {
        error!(target:"rpc", $fmt, $($arg),+);
    };
    (warn $text:expr) => { warn!(
        target: "rpc",
        "{}",
        $text,
    )};
    (warn $fmt:literal, $($arg:expr),+) => {
        warn!(target:"rpc", $fmt, $($arg),+);
    };
    ($text:expr) => {trace!(
        target: "rpc",
        "{}",
        $text,
    )};
    ($fmt:literal, $($arg:expr),+) => {
        trace!(target:"rpc", $fmt, $($arg),+);
    }
}

#[macro_export]
macro_rules! log_rtab {
    (error $text:expr) => { error!(
        target: "rtab",
        "{}",
        $text,
    )};
    (error $fmt:literal, $($arg:expr),+) => {
        error!(target:"rtab", $fmt, $($arg),+);
    };
    (warn $text:expr) => { warn!(
        target: "rtab",
        "{}",
        $text,
    )};
    (warn $fmt:literal, $($arg:expr),+) => {
        warn!(target:"rtab", $fmt, $($arg),+);
    };
    ($text:expr) => {trace!(
        target: "rtab",
        "{}",
        $text,
    )};
    ($fmt:literal, $($arg:expr),+) => {
        trace!(target:"rtab", $fmt, $($arg),+);
    }
}

#[macro_export]
macro_rules! logthru_net {
    ($($level:ident)?) => {
        logthru!($($level)? "net")
    };
    ($($level:ident)? $text:literal) => {
        logthru!($($level)? "net", $text)
    };
    ($($level:ident)? $fmt:literal, $($arg:expr),+) => {
        logthru!($($level)? "net", $fmt, $($arg),+)
    }
}
#[macro_export]
macro_rules! logthru_rpc {
    ($($level:ident)?) => {
        logthru!($($level)? "rpc")
    };
    ($($level:ident)? $text:literal) => {
        logthru!($($level)? "rpc", $text)
    };
    ($($level:ident)? $fmt:literal, $($arg:expr),+) => {
        logthru!($($level)? "rpc", $fmt, $($arg),+)
    }
}
#[macro_export]
macro_rules! logthru_rtab {
    ($($level:ident)?) => {
        logthru!($($level)? "rtab")
    };
    ($($level:ident)? $text:literal) => {
        logthru!($($level)? "rtab", $text)
    };
    ($($level:ident)? $fmt:literal, $($arg:expr),+) => {
        logthru!($($level)? "rtab", $fmt, $($arg),+)
    }
}
#[macro_export]
macro_rules! logthru_pstore {
    ($($level:ident)?) => {
        logthru!($($level)? "pstore")
    };
    ($($level:ident)? $text:literal) => {
        logthru!($($level)? "pstore", $text)
    };
    ($($level:ident)? $fmt:literal, $($arg:expr),+) => {
        logthru!($($level)? "pstore", $fmt, $($arg),+)
    }
}

#[macro_export]
macro_rules! logthru {
    // error
    (error $target:literal) => (|e__| {
        error!(
            target: $target,
            "[{}]",
            e__,
        );
        e__
    });
    (error $target:literal, $text:literal) => (|e__| {
        error!(
            target: $target,
            "[{}] {}",
            e__,
            $text
        );
        e__
    });
    (error $target:literal, $fmt:literal, $($arg:expr),+) => (|e__| {
        error!(
            target: $target,
            concat!("[{}] ", $fmt),
            e__,
            $($arg),+
        );
        e__
    });
    // warn
    (warn $target:literal) => (|e__| {
        warn!(
            target: $target,
            "[{}]",
            e__,
        );
        e__
    });
    (warn $target:literal, $text:literal) => (|e__| {
        warn!(
            target: $target,
            "[{}] {}",
            e__,
            $text
        );
        e__
    });
    (warn $target:literal, $fmt:literal, $($arg:expr),+) => (|e__| {
        warn!(
            target: $target,
            concat!("[{}] ", $fmt),
            e__,
            $($arg),+
        );
        e__
    });
    // debug
    (debug $target:literal) => (|e__| {
        debug!(
            target: $target,
            "[{}]",
            e__,
        );
        e__
    });
    (debug $target:literal, $text:literal) => (|e__| {
        debug!(
            target: $target,
            "[{}] {}",
            e__,
            $text
        );
        e__
    });
    (debug $target:literal, $fmt:literal, $($arg:expr),+) => (|e__| {
        debug!(
            target: $target,
            concat!("[{}] ", $fmt),
            e__,
            $($arg),+
        );
        e__
    });
    // trace
    ($target:literal) => (|e__| {
        trace!(
            target: $target,
            "[{}]",
            e__,
        );
        e__
    });
    ($target:literal, $text:literal) => (|e__| {
        trace!(
            target: $target,
            "[{}] {}",
            e__,
            $text
        );
        e__
    });
    ($target:literal, $fmt:literal, $($arg:expr),+) => (|e__| {
        trace!(
            target: $target,
            concat!("[{}] ", $fmt),
            e__,
            $($arg),+
        );
        e__
    })
}
