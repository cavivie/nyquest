/// A snapshot of response body consumption reported by a streaming response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferProgress {
    /// The number of response body bytes delivered to the caller.
    pub transferred: u64,
    /// The expected response body size, if the backend reported one.
    pub total: Option<u64>,
}
