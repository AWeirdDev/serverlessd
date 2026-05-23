//! Implementation of the serverlessd bindings protocol with `serde` integration.
//!
//! ```no_run
//! let mut client = BindingClient::connect(PathBuf::from(".serverlessd/bindings.sock")).await?;
//! ```

use std::{io, marker::PhantomData, mem, path::PathBuf};

use interprocess::local_socket::{
    ConnectOptions, GenericFilePath,
    tokio::{RecvHalf, SendHalf, prelude::*},
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct Uninitialized;
pub struct Initialized;

#[bon::builder]
pub async fn connect(
    path: PathBuf,
    binding_type: String,
) -> Result<BindingClient<Uninitialized>, BindingClientError> {
    let name = path
        .to_fs_name::<GenericFilePath>()
        .map_err(|err| BindingClientError::NameNotSupported(err))?;

    let (recv, send) = ConnectOptions::new()
        .name(name)
        .connect_tokio()
        .await?
        .split();

    Ok(BindingClient {
        send,
        recv,
        type_: binding_type,
        _phantom: PhantomData,
    })
}

/// Represents a client that speaks to the serverlessd bindings protocol.
pub struct BindingClient<State = Uninitialized> {
    send: SendHalf,
    recv: RecvHalf,

    type_: String,

    _phantom: PhantomData<State>,
}

impl BindingClient<Uninitialized> {
    /// Performs a handshake with the server.
    pub async fn perform_handshake(
        mut self,
    ) -> Result<BindingClient<Initialized>, BindingClientError> {
        let mut buf = vec![];
        extend_raw_str(&mut buf, &self.type_);

        self.send.write_all(&buf).await?;

        tracing::info!("handshake suceeded! connection established");

        // SAFETY: they are the same in-memory layout
        unsafe { mem::transmute(self) }
    }
}

impl BindingClient<Initialized> {
    /// Sends a message to the server.
    pub async fn send_message<T: Serialize>(
        &mut self,
        message: ClientMessage<Result<T, String>>,
    ) -> Result<(), BindingClientError> {
        let payload_raw = serde_json::to_vec(&match message.payload {
            Ok(t) => serde_json::json!({"data": &t}),
            Err(e) => serde_json::json!({"error": &e}),
        })?;

        let mut buf = Vec::with_capacity(size_of::<u32>() * 2 + payload_raw.len());
        buf.extend_from_slice(&message.id.to_le_bytes());
        buf.extend_from_slice(&(payload_raw.len() as u32).to_le_bytes());
        buf.extend_from_slice(&payload_raw);

        self.send.write_all(&buf).await?;

        Ok(())
    }

    #[inline]
    pub async fn send_error<K: ToString>(
        &mut self,
        id: u32,
        message: K,
    ) -> Result<(), BindingClientError> {
        self.send_message::<ijson::IValue>(ClientMessage {
            id,
            payload: Err(message.to_string()),
        })
        .await?;

        Ok(())
    }

    #[inline]
    pub async fn send_ok<T: Serialize>(
        &mut self,
        id: u32,
        payload: T,
    ) -> Result<(), BindingClientError> {
        self.send_message(ClientMessage {
            id,
            payload: Ok(payload),
        })
        .await?;

        Ok(())
    }

    /// Receives a message from the server.
    pub async fn recv_message<T: for<'de> Deserialize<'de>>(
        &mut self,
    ) -> Result<ServerMessage<T>, BindingClientError> {
        let id = self.recv.read_u32_le().await?;
        tracing::info!("got id={id}");

        let data_len = self.recv.read_u32_le().await? as usize;
        let mut data = Box::<[u8]>::new_uninit_slice(data_len);
        let slice =
            unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u8, data_len) };
        self.recv.read_exact(slice).await?;

        let UnprocessedServerMessage {
            func,
            args: data,
            worker,
        } = serde_json::from_slice(slice)?;

        Ok(ServerMessage {
            id,
            function_name: func,
            worker_name: worker,
            args: ijson::from_value::<T>(&data)?,
        })
    }

    // pub async fn shutdown(&mut self) {}
}

#[derive(Debug, thiserror::Error)]
pub enum BindingClientError {
    #[error(transparent)]
    IoError(#[from] io::Error),

    #[error("the socket name is not supported")]
    NameNotSupported(io::Error),

    #[error(transparent)]
    Serialization(#[from] serde_json::Error),

    #[error("invalid function call from the server")]
    InvalidFunctionCall,
}

pub struct ClientMessage<T> {
    /// The identifier of the message.
    pub id: u32,

    /// The payload.
    pub payload: T,
}

#[derive(Deserialize)]
struct UnprocessedServerMessage {
    func: String,
    args: ijson::IValue,
    worker: String,
}

pub struct ServerMessage<T> {
    /// The identifier of the message.
    pub id: u32,

    /// The function to call.
    pub function_name: String,

    /// The name of the worker.
    pub worker_name: String,

    /// The parameters.
    pub args: T,
}

#[inline(always)]
const fn raw_str(s: &str) -> ([u8; size_of::<u32>()], &[u8]) {
    ((s.len() as u32).to_le_bytes(), s.as_bytes())
}

#[inline(always)]
fn extend_raw_str(buf: &mut Vec<u8>, s: &str) {
    let (ln, data) = raw_str(s);
    buf.extend_from_slice(&ln);
    buf.extend_from_slice(data);
}
