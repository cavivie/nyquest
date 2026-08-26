use std::io;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use nyquest_interface::r#async::futures_io;
use nyquest_interface::r#async::AnyAsyncResponse;

use crate::TransferProgress;

/// A [`futures_io::AsyncRead`] stream backed by an async response.
pub struct AsyncReadStream {
    inner: Pin<Box<dyn AnyAsyncResponse>>,
    transferred: u64,
    total: Option<u64>,
    progress: Option<Box<dyn Fn(TransferProgress) + Send + Sync>>,
}

impl AsyncReadStream {
    pub(crate) fn new(
        inner: Pin<Box<dyn AnyAsyncResponse>>,
        total: Option<u64>,
        progress: Option<Box<dyn Fn(TransferProgress) + Send + Sync>>,
    ) -> Self {
        Self {
            inner,
            transferred: 0,
            total,
            progress,
        }
    }
}

impl futures_io::AsyncRead for AsyncReadStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let read = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(read)) = read {
            if read > 0 {
                self.transferred = self.transferred.saturating_add(read as u64);
                if let Some(progress) = &self.progress {
                    progress(TransferProgress {
                        transferred: self.transferred,
                        total: self.total,
                    });
                }
            }
        }
        read
    }
}

mod trait_assert {
    trait _AssertMarker: Send + Sync + Unpin {}
    impl _AssertMarker for super::AsyncReadStream {}
}
