use std::io;

use nyquest_interface::blocking::AnyBlockingResponse;

use crate::TransferProgress;

/// An [`std::io::Read`] stream backed by a blocking response.
pub struct ReadStream {
    inner: Box<dyn AnyBlockingResponse>,
    transferred: u64,
    total: Option<u64>,
    progress: Option<Box<dyn Fn(TransferProgress) + Send + Sync>>,
}

impl ReadStream {
    pub(crate) fn new(
        inner: Box<dyn AnyBlockingResponse>,
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

impl io::Read for ReadStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        if read > 0 {
            self.transferred = self.transferred.saturating_add(read as u64);
            if let Some(progress) = &self.progress {
                progress(TransferProgress {
                    transferred: self.transferred,
                    total: self.total,
                });
            }
        }
        Ok(read)
    }
}

mod trait_assert {
    trait _AssertMarker: Send + Sync {}
    impl _AssertMarker for super::ReadStream {}
}
