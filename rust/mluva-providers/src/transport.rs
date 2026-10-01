use crate::{ProviderError, Result, Secret};
use reqwest::{Client, Response};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub(crate) fn client(timeout: Duration) -> Result<Client> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(timeout)
        .read_timeout(timeout)
        .build()
        .map_err(|_| ProviderError::message("Could not initialize the provider connection."))
}

pub(crate) fn secret_header(value: &Secret) -> Result<reqwest::header::HeaderValue> {
    let mut header = reqwest::header::HeaderValue::from_str(value.value())
        .map_err(|_| ProviderError::message("Could not connect to the configured provider."))?;
    header.set_sensitive(true);
    Ok(header)
}

pub(crate) async fn bounded_body(
    mut response: Response,
    maximum: Option<usize>,
    cancel: &CancellationToken,
) -> std::result::Result<Vec<u8>, ()> {
    let mut data = Vec::new();
    loop {
        let next = tokio::select! {biased; _=cancel.cancelled()=>return Err(()), value=response.chunk()=>value.map_err(|_|())?};
        let Some(chunk) = next else {
            return Ok(data);
        };
        if maximum.is_some_and(|limit| data.len().saturating_add(chunk.len()) > limit) {
            return Err(());
        }
        data.extend_from_slice(&chunk);
    }
}

pub(crate) struct Lines {
    response: Response,
    pending: Vec<u8>,
    offset: usize,
}

impl Lines {
    pub(crate) fn new(response: Response) -> Self {
        Self {
            response,
            pending: Vec::new(),
            offset: 0,
        }
    }
    pub(crate) async fn next(
        &mut self,
        limit: usize,
        cancel: &CancellationToken,
    ) -> std::result::Result<Option<Vec<u8>>, ()> {
        loop {
            let available = &self.pending[self.offset..];
            if let Some(newline) = available.iter().position(|byte| *byte == b'\n') {
                let end = (newline + 1).min(limit);
                let line = available[..end].to_vec();
                self.offset += end;
                return Ok(Some(line));
            }
            if available.len() >= limit {
                let line = available[..limit].to_vec();
                self.offset += limit;
                return Ok(Some(line));
            }
            let next = tokio::select! {biased; _=cancel.cancelled()=>return Err(()),value=self.response.chunk()=>value.map_err(|_|())?};
            match next {
                None if available.is_empty() => return Ok(None),
                None => {
                    let line = available.to_vec();
                    self.offset = self.pending.len();
                    return Ok(Some(line));
                }
                Some(chunk) => {
                    if self.offset != 0 {
                        self.pending.drain(..self.offset);
                        self.offset = 0;
                    }
                    self.pending.extend_from_slice(&chunk);
                }
            }
        }
    }
}
