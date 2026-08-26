use std::fmt;
use std::io;
use std::sync::Arc;

use nyquest_interface::client::ClientOptions;
use nyquest_interface::{Error, Result};

use crate::backend::BackendCore;
use crate::request::{encode_form, prepare_request_parts, set_body, PreparedRequest};
use crate::response::ResponseCore;
use crate::state::wait_for_response_blocking;

#[derive(Clone)]
pub struct AndroidBlockingClient {
    pub(crate) core: Arc<BackendCore>,
    pub(crate) options: ClientOptions,
}

#[cfg(feature = "blocking")]
impl fmt::Debug for AndroidBlockingClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AndroidBlockingClient")
            .field(&"Cronet")
            .finish()
    }
}

#[cfg(feature = "blocking")]
impl nyquest_interface::blocking::BlockingClient for AndroidBlockingClient {
    type Response = AndroidBlockingResponse;

    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }

    fn request(&self, request: nyquest_interface::blocking::Request) -> Result<Self::Response> {
        let prepared = prepare_blocking_request(&self.options, request)?;
        let guard = self.core.start_request(
            prepared,
            self.options.follow_redirects,
            self.options.request_timeout,
        )?;
        let metadata = wait_for_response_blocking(&guard.state)?;
        Ok(AndroidBlockingResponse {
            inner: ResponseCore::new(guard, metadata, self.options.max_response_buffer_size),
        })
    }
}

#[cfg(feature = "blocking")]
fn prepare_blocking_request(
    options: &ClientOptions,
    request: nyquest_interface::blocking::Request,
) -> Result<PreparedRequest> {
    let (mut prepared, body) = prepare_request_parts(options, request)?;
    match body {
        None => {}
        Some(nyquest_interface::Body::Bytes {
            content,
            content_type,
        }) => set_body(&mut prepared, &content_type, content.into_owned()),
        Some(nyquest_interface::Body::Form { fields }) => {
            set_body(
                &mut prepared,
                "application/x-www-form-urlencoded",
                encode_form(fields),
            );
        }
        Some(nyquest_interface::Body::Stream {
            mut stream,
            content_type,
        }) => {
            #[cfg(feature = "blocking-stream")]
            {
                use std::io::Read;
                let mut body = Vec::new();
                stream.read_to_end(&mut body)?;
                set_body(&mut prepared, &content_type, body);
            }
            #[cfg(not(feature = "blocking-stream"))]
            match stream {}
        }
    }
    Ok(prepared)
}

#[cfg(feature = "blocking")]
pub struct AndroidBlockingResponse {
    inner: ResponseCore,
}

#[cfg(feature = "blocking")]
impl nyquest_interface::blocking::BlockingResponse for AndroidBlockingResponse {
    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AndroidBlockingResponse")
            .field("status", &self.inner.metadata.status)
            .finish()
    }

    fn status(&self) -> u16 {
        self.inner.metadata.status
    }

    fn content_length(&self) -> Option<u64> {
        self.inner.metadata.content_length
    }

    fn get_header(&self, header: &str) -> Result<Vec<String>> {
        Ok(self.inner.metadata.header(header))
    }

    fn text(&mut self) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.bytes()?).into_owned())
    }

    fn bytes(&mut self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        while let Some(chunk) = self.inner.next_chunk_blocking()? {
            if self
                .inner
                .max_response_buffer_size
                .is_some_and(|max| bytes.len() + chunk.len() > max as usize)
            {
                return Err(Error::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

#[cfg(feature = "blocking-stream")]
impl io::Read for AndroidBlockingResponse {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.inner.read_blocking(buffer)
    }
}
