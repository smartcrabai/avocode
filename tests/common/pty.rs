//! PTY-based TUI automation helpers.
//!
//! Uses `portable-pty` to spawn `avocode` under a pseudo-terminal and `vt100`
//! to decode the terminal output into a parseable screen buffer.
#![allow(dead_code)]
#![expect(clippy::expect_used)]

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const PTY_COLS: u16 = 120;
pub const PTY_ROWS: u16 = 40;

/// Drives an `avocode` TUI session running inside a PTY.
pub struct TuiDriver {
    writer: Box<dyn Write + Send>,
    _child: Box<dyn portable_pty::Child + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    _reader_thread: std::thread::JoinHandle<()>,
}

impl TuiDriver {
    /// Spawn `avocode` (no-argument, TUI mode) inside a PTY with the given
    /// environment overrides and working directory.
    pub fn spawn(env_overrides: &[(String, String)], cwd: &std::path::Path) -> Self {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: PTY_ROWS,
                cols: PTY_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("failed to open PTY pair");

        let mut cmd = CommandBuilder::new(super::process::AVOCODE_BIN);
        cmd.cwd(cwd);
        for (k, v) in env_overrides {
            cmd.env(k, v);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .expect("failed to spawn avocode in PTY");

        // Take the writer once and store it for reuse across multiple `send_input` calls.
        let writer = pair
            .master
            .take_writer()
            .expect("failed to take PTY writer");

        let parser = Arc::new(Mutex::new(vt100::Parser::new(PTY_ROWS, PTY_COLS, 0)));
        let parser_clone = Arc::clone(&parser);

        let mut reader = pair
            .master
            .try_clone_reader()
            .expect("failed to clone PTY reader");

        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        parser_clone
                            .lock()
                            .expect("parser lock poisoned")
                            .process(&buf[..n]);
                    }
                }
            }
        });

        TuiDriver {
            writer,
            _child: child,
            parser,
            _reader_thread: reader_thread,
        }
    }

    /// Send a string as keyboard input to the running TUI.
    pub fn send_input(&mut self, text: &str) {
        self.writer
            .write_all(text.as_bytes())
            .expect("failed to write to PTY");
    }

    /// Send `Ctrl+C` to quit the TUI.
    pub fn send_ctrl_c(&mut self) {
        self.send_input("\x03");
        // Flush to ensure the signal reaches the child process.
        let _ = self.writer.flush();
    }

    /// Return the current visible screen contents as a plain string.
    pub fn screen_contents(&self) -> String {
        self.parser
            .lock()
            .expect("parser lock poisoned")
            .screen()
            .contents()
    }

    /// Poll the screen until `predicate` returns `true` or `timeout` elapses.
    ///
    /// Returns `true` if the condition was satisfied, `false` on timeout.
    pub fn wait_for<F>(&self, predicate: F, timeout: Duration) -> bool
    where
        F: Fn(&str) -> bool,
    {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if predicate(&self.screen_contents()) {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
