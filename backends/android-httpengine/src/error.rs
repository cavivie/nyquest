use std::io;

use jni::Env;
use nyquest_interface::Error;

#[derive(Clone)]
pub(crate) enum Failure {
    Timeout,
    Io(String),
}

impl From<jni::errors::Error> for Failure {
    fn from(error: jni::errors::Error) -> Self {
        Self::Io(format!("Android JNI call failed: {error}"))
    }
}

impl Failure {
    pub(crate) fn into_error(self) -> Error {
        match self {
            Self::Timeout => Error::RequestTimeout,
            Self::Io(message) => Error::Io(io::Error::other(message)),
        }
    }
}

pub(crate) fn java_failure(env: &mut Env<'_>, context: &str, error: jni::errors::Error) -> Failure {
    clear_java_exception(env);
    Failure::Io(format!("{context}: {error}"))
}

pub(crate) fn clear_java_exception(env: &mut Env<'_>) {
    if env.exception_check() {
        env.exception_describe();
        env.exception_clear();
    }
}

pub(crate) fn jni_io(context: &str, error: jni::errors::Error) -> Error {
    Error::Io(io::Error::other(format!("{context}: {error}")))
}

pub(crate) fn unsupported(message: &str) -> Error {
    Error::Io(io::Error::new(io::ErrorKind::Unsupported, message))
}

pub(crate) fn error_to_io(error: Error) -> io::Error {
    match error {
        Error::Io(error) => error,
        Error::RequestTimeout => io::Error::new(io::ErrorKind::TimedOut, error),
        other => io::Error::other(other),
    }
}
