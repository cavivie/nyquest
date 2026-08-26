//! `async` client support.

use std::borrow::Cow;
#[cfg(feature = "async-stream")]
use std::io::{self, SeekFrom};
#[cfg(feature = "async-stream")]
use std::pin::Pin;
#[cfg(feature = "async-stream")]
use std::task::{Context, Poll};

#[cfg(feature = "async-stream")]
use nyquest_interface::r#async::{
    futures_io::{AsyncRead, AsyncSeek},
    BoxedStream, SizedBodyStream, UnsizedBodyStream,
};

#[cfg(feature = "async-stream")]
mod async_read_stream;
pub(crate) mod client;
mod response;

#[cfg(not(feature = "async-stream"))]
type BoxedStream = std::convert::Infallible;

/// The Request Body type for async requests.
pub type Body = crate::body::Body<BoxedStream>;
/// The Request type for async requests.
pub type Request = crate::Request<BoxedStream>;
/// The multipart form part type for async requests.
#[cfg(feature = "multipart")]
pub type Part = crate::body::Part<BoxedStream>;
/// The multipart form part body type for async requests.
#[cfg(feature = "multipart")]
pub type PartBody = crate::body::PartBody<BoxedStream>;
#[cfg(feature = "async-stream")]
pub use async_read_stream::AsyncReadStream;
pub use response::Response;

#[cfg(feature = "async-stream")]
use crate::body::private::{
    IntoSizedStream, IntoSizedStreamWithProgress, IntoUnsizedStream, IntoUnsizedStreamWithProgress,
    ProgressObserver,
};
#[cfg(feature = "async-stream")]
use crate::TransferProgress;

/// Shortcut method to quickly make a `GET` request.
///
/// See also the methods on the [`Response`] type.
///
/// **Note**: This function creates a new internal [`AsyncClient`] on each call, and so should not
/// be used if making many requests. Create a [`AsyncClient`] instead.
///
/// [`AsyncClient`]: crate::AsyncClient
pub async fn get(uri: impl Into<Cow<'static, str>>) -> crate::Result<Response> {
    let client = crate::client::ClientBuilder::default()
        .build_async()
        .await?;
    client.request(Request::get(uri)).await
}

#[cfg(feature = "async-stream")]
impl<S: SizedBodyStream> IntoSizedStream<BoxedStream> for S {
    fn into_stream(self, size: u64) -> BoxedStream {
        BoxedStream::Sized {
            stream: Box::pin(self),
            content_length: size,
        }
    }
}

#[cfg(feature = "async-stream")]
impl<S: UnsizedBodyStream> IntoUnsizedStream<BoxedStream> for S {
    fn into_stream(self) -> BoxedStream {
        BoxedStream::Unsized {
            stream: Box::pin(self),
        }
    }
}

#[cfg(feature = "async-stream")]
struct ProgressStream<S: ?Sized> {
    inner: Pin<Box<S>>,
    transferred: u64,
    total: Option<u64>,
    observer: ProgressObserver,
}

#[cfg(feature = "async-stream")]
impl<S: ?Sized> ProgressStream<S> {
    fn new(inner: Pin<Box<S>>, total: Option<u64>, observer: ProgressObserver) -> Self {
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

#[cfg(feature = "async-stream")]
impl<S: AsyncRead + ?Sized> AsyncRead for ProgressStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match this.inner.as_mut().poll_read(cx, buf) {
            Poll::Ready(Ok(read)) if read > 0 => {
                this.transferred = this.transferred.saturating_add(read as u64);
                this.report();
                Poll::Ready(Ok(read))
            }
            result => result,
        }
    }
}

#[cfg(feature = "async-stream")]
impl<S: AsyncSeek + ?Sized> AsyncSeek for ProgressStream<S> {
    fn poll_seek(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        position: SeekFrom,
    ) -> Poll<io::Result<u64>> {
        let this = self.get_mut();
        match this.inner.as_mut().poll_seek(cx, position) {
            Poll::Ready(Ok(position)) => {
                this.transferred = position;
                this.report();
                Poll::Ready(Ok(position))
            }
            result => result,
        }
    }
}

#[cfg(feature = "async-stream")]
impl<S: SizedBodyStream> IntoSizedStreamWithProgress<BoxedStream> for S {
    fn into_stream_with_progress(self, size: u64, observer: ProgressObserver) -> BoxedStream {
        BoxedStream::Sized {
            stream: Box::pin(ProgressStream::new(Box::pin(self), Some(size), observer)),
            content_length: size,
        }
    }
}

#[cfg(feature = "async-stream")]
impl<S: UnsizedBodyStream> IntoUnsizedStreamWithProgress<BoxedStream> for S {
    fn into_stream_with_progress(self, observer: ProgressObserver) -> BoxedStream {
        BoxedStream::Unsized {
            stream: Box::pin(ProgressStream::new(Box::pin(self), None, observer)),
        }
    }
}
