use std::io;
use std::sync::Arc;
#[cfg(feature = "async")]
use std::task::Poll;

use nyquest_interface::Result;

use crate::error::{error_to_io, Failure};
use crate::state::{RequestGuard, RequestState, RequestStateData, ResponseMetadata};

pub(crate) struct ResponseCore {
    _guard: RequestGuard,
    pub(crate) metadata: ResponseMetadata,
    current_chunk: Vec<u8>,
    current_offset: usize,
    pub(crate) max_response_buffer_size: Option<u64>,
}

impl ResponseCore {
    pub(crate) fn new(
        guard: RequestGuard,
        metadata: ResponseMetadata,
        max_response_buffer_size: Option<u64>,
    ) -> Self {
        Self {
            _guard: guard,
            metadata,
            current_chunk: Vec::new(),
            current_offset: 0,
            max_response_buffer_size,
        }
    }

    fn state(&self) -> &Arc<RequestState> {
        &self._guard.state
    }

    fn take_buffered(&mut self, output: &mut [u8]) -> usize {
        let remaining = &self.current_chunk[self.current_offset..];
        let count = output.len().min(remaining.len());
        output[..count].copy_from_slice(&remaining[..count]);
        self.current_offset += count;
        if self.current_offset == self.current_chunk.len() {
            self.current_chunk.clear();
            self.current_offset = 0;
        }
        count
    }

    fn begin_read_if_needed(&self, data: &mut RequestStateData) -> bool {
        if data.read_in_flight || data.terminal.is_some() {
            false
        } else {
            data.read_in_flight = true;
            true
        }
    }

    #[cfg(feature = "async")]
    pub(crate) fn poll_chunk(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<Option<Vec<u8>>>> {
        let should_read = {
            let mut data = self.state().lock();
            if let Some(chunk) = data.chunks.pop_front() {
                return Poll::Ready(Ok(Some(chunk)));
            }
            if let Some(terminal) = data.terminal.clone() {
                return Poll::Ready(terminal.map(|()| None).map_err(Failure::into_error));
            }
            data.read_waker = Some(cx.waker().clone());
            self.begin_read_if_needed(&mut data)
        };
        if should_read {
            if let Err(failure) = self.state().request_read() {
                self.state().fail(failure);
                return self.poll_chunk(cx);
            }
        }
        Poll::Pending
    }

    #[cfg(feature = "async-stream")]
    pub(crate) fn poll_read(
        &mut self,
        cx: &mut std::task::Context<'_>,
        output: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let count = self.take_buffered(output);
        if count > 0 || output.is_empty() {
            return Poll::Ready(Ok(count));
        }
        match self.poll_chunk(cx) {
            Poll::Ready(Ok(Some(chunk))) => {
                self.current_chunk = chunk;
                Poll::Ready(Ok(self.take_buffered(output)))
            }
            Poll::Ready(Ok(None)) => Poll::Ready(Ok(0)),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error_to_io(error))),
            Poll::Pending => Poll::Pending,
        }
    }

    #[cfg(feature = "blocking")]
    pub(crate) fn next_chunk_blocking(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            let should_read = {
                let mut data = self.state().lock();
                if let Some(chunk) = data.chunks.pop_front() {
                    return Ok(Some(chunk));
                }
                if let Some(terminal) = data.terminal.clone() {
                    return terminal.map(|()| None).map_err(Failure::into_error);
                }
                self.begin_read_if_needed(&mut data)
            };
            if should_read {
                if let Err(failure) = self.state().request_read() {
                    self.state().fail(failure);
                    continue;
                }
            }
            let data = self.state().lock();
            drop(
                self.state()
                    .changed
                    .wait(data)
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            );
        }
    }

    #[cfg(feature = "blocking-stream")]
    pub(crate) fn read_blocking(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = self.take_buffered(output);
        if count > 0 || output.is_empty() {
            return Ok(count);
        }
        match self.next_chunk_blocking().map_err(error_to_io)? {
            Some(chunk) => {
                self.current_chunk = chunk;
                Ok(self.take_buffered(output))
            }
            None => Ok(0),
        }
    }
}
