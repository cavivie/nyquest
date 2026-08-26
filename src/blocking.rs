//! Blocking client support.
//!
//! The blocking client will block the current thread to execute.
//!

use std::borrow::Cow;
#[cfg(feature = "blocking-stream")]
use std::io::{Read, Seek, SeekFrom};

#[cfg(feature = "blocking-stream")]
use nyquest_interface::blocking::{BoxedStream, SizedBodyStream, UnsizedBodyStream};

pub(crate) mod client;
#[cfg(feature = "blocking-stream")]
mod read_stream;
mod response;

#[cfg(not(feature = "blocking-stream"))]
type BoxedStream = std::convert::Infallible;

/// The Request Body type for blocking requests.
pub type Body = crate::body::Body<BoxedStream>;
/// The Request type for blocking requests.
pub type Request = crate::Request<BoxedStream>;
/// The multipart form part type for blocking requests.
#[cfg(feature = "multipart")]
pub type Part = crate::body::Part<BoxedStream>;
/// The multipart form part body type for blocking requests.
#[cfg(feature = "multipart")]
pub type PartBody = crate::body::PartBody<BoxedStream>;
#[cfg(feature = "blocking-stream")]
pub use read_stream::ReadStream;
pub use response::Response;

#[cfg(feature = "blocking-stream")]
use crate::body::private::{
    IntoSizedStream, IntoSizedStreamWithProgress, IntoUnsizedStream, IntoUnsizedStreamWithProgress,
    ProgressObserver,
};
#[cfg(feature = "blocking-stream")]
use crate::TransferProgress;

/// Shortcut method to quickly make a `GET` request.
///
/// See also the methods on the [`Response`] type.
///
/// **Note**: This function creates a new internal [`BlockingClient`] on each call, and so should
/// not be used if making many requests. Create a [`BlockingClient`] instead.
///
/// [`BlockingClient`]: crate::BlockingClient
pub fn get(uri: impl Into<Cow<'static, str>>) -> crate::Result<Response> {
    let client = crate::client::ClientBuilder::default().build_blocking()?;
    client.request(Request::get(uri))
}

#[cfg(feature = "blocking-stream")]
impl<S: SizedBodyStream> IntoSizedStream<BoxedStream> for S {
    fn into_stream(self, size: u64) -> BoxedStream {
        BoxedStream::Sized {
            stream: Box::new(self),
            content_length: size,
        }
    }
}

#[cfg(feature = "blocking-stream")]
impl<S: UnsizedBodyStream> IntoUnsizedStream<BoxedStream> for S {
    fn into_stream(self) -> BoxedStream {
        BoxedStream::Unsized {
            stream: Box::new(self),
        }
    }
}

#[cfg(feature = "blocking-stream")]
struct ProgressStream<S: ?Sized> {
    inner: Box<S>,
    transferred: u64,
    total: Option<u64>,
    observer: ProgressObserver,
}

#[cfg(feature = "blocking-stream")]
impl<S: ?Sized> ProgressStream<S> {
    fn new(inner: Box<S>, total: Option<u64>, observer: ProgressObserver) -> Self {
        Self {
            inner,
            transferred: 0,
            total,
            observer,
        }
    }

    fn report(&self) {
        (self.observer)(TransferProgress {
            transferred: self.transferred,
            total: self.total,
        });
    }
}

#[cfg(feature = "blocking-stream")]
impl<S: Read + ?Sized> Read for ProgressStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        if read > 0 {
            self.transferred = self.transferred.saturating_add(read as u64);
            self.report();
        }
        Ok(read)
    }
}

#[cfg(feature = "blocking-stream")]
impl<S: Seek + ?Sized> Seek for ProgressStream<S> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let position = self.inner.seek(position)?;
        self.transferred = position;
        self.report();
        Ok(position)
    }
}

#[cfg(feature = "blocking-stream")]
impl<S: SizedBodyStream> IntoSizedStreamWithProgress<BoxedStream> for S {
    fn into_stream_with_progress(self, size: u64, observer: ProgressObserver) -> BoxedStream {
        BoxedStream::Sized {
            stream: Box::new(ProgressStream::new(Box::new(self), Some(size), observer)),
            content_length: size,
        }
    }
}

#[cfg(feature = "blocking-stream")]
impl<S: UnsizedBodyStream> IntoUnsizedStreamWithProgress<BoxedStream> for S {
    fn into_stream_with_progress(self, observer: ProgressObserver) -> BoxedStream {
        BoxedStream::Unsized {
            stream: Box::new(ProgressStream::new(Box::new(self), None, observer)),
        }
    }
}
