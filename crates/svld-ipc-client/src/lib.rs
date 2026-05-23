//! Implementation of the serverlessd bindings protocol with `serde` integration.
//!
//! ```no_run
//! let mut client = BindingClient::connect().await?;
//! ```

use std::{
    io::{self, IoSlice},
    path::PathBuf,
};

use interprocess::local_socket::{
    ConnectOptions, GenericFilePath,
    tokio::{RecvHalf, SendHalf, prelude::*},
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Represents a client that speaks to the serverlessd bindings protocol.
pub struct BindingClient {
    send: SendHalf,
    recv: RecvHalf,
}

impl BindingClient {
    pub async fn connect(path: PathBuf) -> Result<Self, BindingClientError> {
        let name = path
            .to_fs_name::<GenericFilePath>()
            .map_err(|err| BindingClientError::NameNotSupported(err))?;

        let (recv, send) = ConnectOptions::new()
            .name(name)
            .connect_tokio()
            .await?
            .split();

        Ok(Self { send, recv })
    }

    /// Sends a message to the server.
    pub async fn send_message<T: Serialize>(
        &mut self,
        message: Message<T>,
    ) -> Result<(), BindingClientError> {
        let id_raw = message.id.to_le_bytes();
        let payload_raw = serde_json::to_vec(&message.payload)?;
        let len_raw = (payload_raw.len() as u32).to_le_bytes();

        let slices = [
            IoSlice::new(&id_raw),
            IoSlice::new(&len_raw),
            IoSlice::new(&payload_raw),
        ];

        self.send.write_vectored(&slices).await?;

        Ok(())
    }

    /// Receives a message from the server.
    pub async fn recv_message<T: for<'de> Deserialize<'de>>(
        &mut self,
    ) -> Result<Message<T>, BindingClientError> {
        let id = self.recv.read_u32_le().await?;

        let data_len = self.recv.read_u32_le().await? as usize;
        let mut data = Box::<[u8]>::new_uninit_slice(data_len);
        let slice =
            unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u8, data_len) };
        self.recv.read_exact(slice).await?;

        let payload = serde_json::from_slice::<T>(slice)?;

        Ok(Message { id, payload })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BindingClientError {
    #[error(transparent)]
    IoError(#[from] io::Error),

    #[error("the socket name is not supported")]
    NameNotSupported(io::Error),

    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}

pub struct Message<T> {
    /// The identifier of the message.
    pub id: u32,

    /// The payload.
    pub payload: T,
}
