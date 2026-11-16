use std::error::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetFailure {
    Unreachable,
    Tls,
    Timeout,
    Server,
    TorConnecting,
    TorFailed,
}

impl From<crate::tor::Blocked> for NetFailure {
    fn from(b: crate::tor::Blocked) -> Self {
        match b {
            crate::tor::Blocked::Connecting => NetFailure::TorConnecting,
            crate::tor::Blocked::Failed => NetFailure::TorFailed,
        }
    }
}

impl NetFailure {
    pub fn of(e: &(dyn Error + 'static)) -> Self {
        let mut timeout = false;
        let mut current = Some(e);
        while let Some(err) = current {
            if is_tls(err) {
                return NetFailure::Tls;
            }
            timeout |= err.is::<tonic::TimeoutExpired>();
            if let Some(status) = err.downcast_ref::<tonic::Status>() {
                timeout |= status.code() == tonic::Code::DeadlineExceeded;
            }
            if let Some(r) = err.downcast_ref::<reqwest::Error>() {
                timeout |= r.is_timeout();
            }
            if let Some(zcash_client_backend::tor::http::HttpError::Timeout(_)) =
                err.downcast_ref::<zcash_client_backend::tor::http::HttpError>()
            {
                timeout = true;
            }
            current = match err.downcast_ref::<std::io::Error>() {
                Some(io) => {
                    timeout |= io.kind() == std::io::ErrorKind::TimedOut;
                    match io.get_ref() {
                        Some(inner) => Some(inner as &(dyn Error + 'static)),
                        None => err.source(),
                    }
                }
                None => err.source(),
            };
        }
        if timeout {
            NetFailure::Timeout
        } else {
            NetFailure::Unreachable
        }
    }

    pub fn of_status(status: &tonic::Status) -> Self {
        use tonic::Code;
        match status.code() {
            Code::Cancelled => match Self::of(status) {
                NetFailure::Unreachable => NetFailure::Timeout,
                f => f,
            },
            Code::Unavailable | Code::Unknown | Code::DeadlineExceeded => Self::of(status),
            _ => NetFailure::Server,
        }
    }
}

fn is_tls(e: &(dyn Error + 'static)) -> bool {
    e.is::<rustls::Error>()
}

pub fn error_chain(e: &dyn Error) -> String {
    let mut message = e.to_string();
    let mut source = e.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !message.ends_with(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_io_causes() {
        let refused = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        assert_eq!(NetFailure::of(&refused), NetFailure::Unreachable);
        let timed_out = std::io::Error::new(std::io::ErrorKind::TimedOut, "slow");
        assert_eq!(NetFailure::of(&timed_out), NetFailure::Timeout);
        let tls = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer),
        );
        assert_eq!(NetFailure::of(&tls), NetFailure::Tls);
        let nested = std::io::Error::other(tls);
        assert_eq!(NetFailure::of(&nested), NetFailure::Tls);
    }

    #[test]
    fn classifies_statuses() {
        assert_eq!(
            NetFailure::of_status(&tonic::Status::not_found("no such block")),
            NetFailure::Server
        );
        assert_eq!(
            NetFailure::of_status(&tonic::Status::unavailable("down")),
            NetFailure::Unreachable
        );
        assert_eq!(
            NetFailure::of_status(&tonic::Status::deadline_exceeded("slow")),
            NetFailure::Timeout
        );
        assert_eq!(
            NetFailure::of_status(&tonic::Status::cancelled("Timeout expired")),
            NetFailure::Timeout
        );
    }
}
