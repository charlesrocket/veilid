use parking_lot::Mutex;
use std::sync::Arc;

// Must use async_std channel to send to main thread from blocking thread
use async_std::channel::bounded as async_bounded;
use async_std::channel::Receiver as AsyncReceiver;
pub use async_std::channel::RecvError;

// Must use std mpsc so no logs are generated by async code
use std::sync::mpsc::sync_channel as std_sync_channel;
use std::sync::mpsc::SyncSender as StdSender;
use std::sync::mpsc::TrySendError as StdTrySendError;

//////////////////////////////////////////

pub struct ClientLogChannelCloser {
    sender: Arc<Mutex<Option<StdSender<String>>>>,
}

impl ClientLogChannelCloser {
    pub fn close(&self) {
        // Drop the sender
        self.sender.lock().take();
    }
}

//////////////////////////////////////////
pub struct ClientLogChannelWriterShim {
    sender: Arc<Mutex<Option<StdSender<String>>>>,
}

impl std::io::Write for ClientLogChannelWriterShim {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let bufstr = String::from_utf8_lossy(buf).to_string();
        let sender = self.sender.lock();
        if let Some(sender) = &*sender {
            if let Err(e) = sender.try_send(bufstr) {
                match e {
                    StdTrySendError::Full(_) => {
                        Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
                    }
                    StdTrySendError::Disconnected(_) => {
                        Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
                    }
                }
            } else {
                Ok(buf.len())
            }
        } else {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub type ClientLogChannelWriter = std::io::LineWriter<ClientLogChannelWriterShim>;

//////////////////////////////////////////

pub struct ClientLogChannel {
    async_receiver: AsyncReceiver<String>,
}

impl ClientLogChannel {
    pub fn new() -> (Self, ClientLogChannelWriter, ClientLogChannelCloser) {
        let (async_sender, async_receiver) = async_bounded(1024);
        let (std_sender, std_receiver) = std_sync_channel(1024);
        let shared_std_sender = Arc::new(Mutex::new(Some(std_sender)));

        // Spawn a processing thread for the blocking std sender
        async_std::task::spawn(async move {
            #[allow(clippy::while_let_loop)]
            loop {
                let message = match std_receiver.recv() {
                    Ok(v) => v,
                    Err(_) => break,
                };
                if async_sender.send(message).await.is_err() {
                    break;
                }
            }
        });

        (
            Self { async_receiver },
            ClientLogChannelWriter::with_capacity(
                65536,
                ClientLogChannelWriterShim {
                    sender: shared_std_sender.clone(),
                },
            ),
            ClientLogChannelCloser {
                sender: shared_std_sender,
            },
        )
    }

    pub async fn recv(&mut self) -> Result<String, RecvError> {
        self.async_receiver.recv().await
    }
}
