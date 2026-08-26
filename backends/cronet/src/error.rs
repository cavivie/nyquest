use std::io;

use jni::JNIEnv;
use nyquest_interface::Error;

#[derive(Clone)]
pub(crate) enum Failure {
    Timeout,
    Io(String),
}

impl Failure {
    pub(crate) fn into_error(self) -> Error {
        match self {
            Self::Timeout => Error::RequestTimeout,
            Self::Io(message) => Error::Io(io::Error::other(message)),
        }
    }
}

pub(crate) fn java_error(env: &mut JNIEnv<'_>, context: &str, error: jni::errors::Error) -> Error {
    Error::Io(io::Error::other(
        java_failure(env, context, error).message(),
    ))
}

pub(crate) fn java_failure(
    env: &mut JNIEnv<'_>,
    context: &str,
    error: jni::errors::Error,
) -> Failure {
    clear_java_exception(env);
    Failure::Io(format!("{context}: {error}"))
}

impl Failure {
    pub(crate) fn message(self) -> String {
        match self {
            Self::Timeout => "request timed out".into(),
            Self::Io(message) => message,
        }
    }
}

pub(crate) fn clear_java_exception(env: &mut JNIEnv<'_>) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
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
