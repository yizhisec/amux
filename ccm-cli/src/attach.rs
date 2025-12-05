//! Attach mode - raw terminal connection to session

use crate::client::Client;
use crate::error::AttachError;
use ccm_proto::daemon::AttachInput;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, size},
};
use std::io::{self, Write};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[allow(dead_code)]
type Result<T> = std::result::Result<T, AttachError>;

/// Attach to a session
#[allow(dead_code)]
pub async fn attach(client: &mut Client, session_id: &str) -> Result<()> {
    // Get terminal size
    let (cols, rows) = size().map_err(AttachError::Terminal)?;

    // Setup raw terminal
    enable_raw_mode().map_err(AttachError::Terminal)?;

    // Create channels for input
    let (tx, rx) = mpsc::channel::<AttachInput>(32);

    // Send initial message with session ID and size
    tx.send(AttachInput {
        session_id: session_id.to_string(),
        data: vec![],
        rows: Some(rows as u32),
        cols: Some(cols as u32),
    })
    .await
    .map_err(|_| AttachError::ChannelSend)?;

    // Start attach stream
    let response = client
        .inner_mut()
        .attach_session(ReceiverStream::new(rx))
        .await?;

    let mut output_stream = response.into_inner();

    // Spawn task to read from output stream and write to stdout
    let output_handle = tokio::spawn(async move {
        let mut stdout = io::stdout();
        while let Ok(Some(msg)) = output_stream.message().await {
            stdout.write_all(&msg.data).ok();
            stdout.flush().ok();
        }
    });

    // Read input and send to session
    let session_id_clone = session_id.to_string();
    let input_loop = async {
        loop {
            if event::poll(std::time::Duration::from_millis(10)).map_err(AttachError::Terminal)? {
                match event::read().map_err(AttachError::Terminal)? {
                    Event::Key(key) => {
                        // Check for detach key (Ctrl+])
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char(']')
                        {
                            break;
                        }

                        // Convert key to bytes
                        let data = key_to_bytes(&key);
                        if !data.is_empty() {
                            tx.send(AttachInput {
                                session_id: session_id_clone.clone(),
                                data,
                                rows: None,
                                cols: None,
                            })
                            .await
                            .ok();
                        }
                    }
                    Event::Resize(cols, rows) => {
                        tx.send(AttachInput {
                            session_id: session_id_clone.clone(),
                            data: vec![],
                            rows: Some(rows as u32),
                            cols: Some(cols as u32),
                        })
                        .await
                        .ok();
                    }
                    _ => {}
                }
            }
        }
        Ok::<(), AttachError>(())
    };

    let _ = input_loop.await;

    // Cleanup
    output_handle.abort();
    disable_raw_mode().map_err(AttachError::Terminal)?;

    println!("\n[Detached from session]");

    Ok(())
}

/// Convert a key event to bytes to send to PTY
#[allow(dead_code)]
fn key_to_bytes(key: &crossterm::event::KeyEvent) -> Vec<u8> {
    use KeyCode::*;

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let Char(c) = key.code {
            // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
            let ctrl_char = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a' - 1);
            return vec![ctrl_char];
        }
    }

    match key.code {
        Char(c) => c.to_string().into_bytes(),
        Enter => vec![b'\r'],
        Tab => vec![b'\t'],
        Backspace => vec![0x7f],
        Esc => vec![0x1b],
        Up => b"\x1b[A".to_vec(),
        Down => b"\x1b[B".to_vec(),
        Right => b"\x1b[C".to_vec(),
        Left => b"\x1b[D".to_vec(),
        Home => b"\x1b[H".to_vec(),
        End => b"\x1b[F".to_vec(),
        PageUp => b"\x1b[5~".to_vec(),
        PageDown => b"\x1b[6~".to_vec(),
        Delete => b"\x1b[3~".to_vec(),
        Insert => b"\x1b[2~".to_vec(),
        F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}
