use std::fmt;
use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

use nyquest_interface::client::ClientOptions;
use nyquest_interface::{Error, Result};

use crate::backend::BackendCore;
use crate::request::{encode_form, prepare_request_parts, set_body, PreparedRequest};
use crate::response::ResponseCore;
use crate::state::wait_for_response_async;

#[cfg(feature = "async")]
#[derive(Clone)]
pub struct AndroidAsyncClient {
    pub(crate) core: Arc<BackendCore>,
    pub(crate) options: ClientOptions,
}

#[cfg(feature = "async")]
impl fmt::Debug for AndroidAsyncClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AndroidAsyncClient")
            .field(&self.core.bindings.name)
            .finish()
    }
}

#[cfg(feature = "async")]
impl nyquest_interface::r#async::AsyncClient for AndroidAsyncClient {
    type Response = AndroidAsyncResponse;

    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }

    async fn request(
        &self,
        request: nyquest_interface::r#async::Request,
    ) -> Result<Self::Response> {
        let prepared = prepare_async_request(&self.options, request).await?;
        let guard = self.core.start_request(
            prepared,
            self.options.follow_redirects,
            self.options.request_timeout,
        )?;
        let metadata = wait_for_response_async(&guard.state).await?;
        Ok(AndroidAsyncResponse {
            inner: ResponseCore::new(guard, metadata, self.options.max_response_buffer_size),
        })
    }
}

#[cfg(feature = "async")]
async fn prepare_async_request(
    options: &ClientOptions,
    request: nyquest_interface::r#async::Request,
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
            stream,
            content_type,
        }) => {
            #[cfg(feature = "async-stream")]
            {
                use futures_util::AsyncReadExt;
                let mut stream = stream;
                let mut body = Vec::new();
                stream.read_to_end(&mut body).await?;
                set_body(&mut prepared, &content_type, body);
            }
            #[cfg(not(feature = "async-stream"))]
            match stream {}
        }
    }
    Ok(prepared)
}

#[cfg(feature = "async")]
pub struct AndroidAsyncResponse {
    inner: ResponseCore,
}

#[cfg(feature = "async")]
impl nyquest_interface::r#async::AsyncResponse for AndroidAsyncResponse {
    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AndroidAsyncResponse")
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

    async fn text(mut self: Pin<&mut Self>) -> Result<String> {
        let bytes = self.as_mut().bytes().await?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn bytes(mut self: Pin<&mut Self>) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        loop {
            let chunk = poll_fn(|cx| self.inner.poll_chunk(cx)).await?;
            let Some(chunk) = chunk else { break };
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

#[cfg(feature = "async-stream")]
impl nyquest_interface::r#async::futures_io::AsyncRead for AndroidAsyncResponse {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_read(cx, buffer)
    }
}
