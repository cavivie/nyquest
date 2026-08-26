<div class="rustdoc-hidden">

# nyquest-backend-nsurlsession

</div>

[`NSURLSession`](https://developer.apple.com/documentation/foundation/nsurlsession) backend for [`nyquest`].

## Features

- `blocking`
- `blocking-stream`
- `async`
- `async-stream`
- `multipart`

## Cancellation

Dropping an asynchronous request future before response headers arrive cancels its underlying
`NSURLSessionDataTask`. After headers arrive, ownership of the task moves to the response.

[`nyquest`]: https://docs.rs/nyquest
