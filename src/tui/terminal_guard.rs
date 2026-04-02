//! RAII guard for terminal raw-mode and alternate-screen lifecycle.
//!
//! Inspired by `codex-rs/tui/src/tui.rs` (init/restore split):
//! entering raw mode + alternate screen happens in [`TerminalGuard::init`],
//! and the corresponding cleanup runs automatically on `Drop`.

use std::io;

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

/// Owning wrapper that guarantees terminal restore on drop.
///
/// Call [`TerminalGuard::init`] to enter raw mode and alternate screen.
/// When the guard is dropped, raw mode is disabled and the alternate screen
/// is left automatically.
pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    /// Tracks whether `Drop` still needs to run cleanup.
    /// Set to `false` by [`Self::restore`] so that a double-restore is harmless.
    needs_restore: bool,
}

impl TerminalGuard {
    /// Enter raw mode and alternate screen, returning a guarded terminal.
    ///
    /// # Errors
    ///
    /// Returns an error if `enable_raw_mode` or `EnterAlternateScreen` fails.
    pub fn init() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(e) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(e);
        }
        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self {
                terminal,
                needs_restore: true,
            }),
            Err(e) => {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                Err(e)
            }
        }
    }

    /// Manually restore the terminal and consume the guard.
    ///
    /// Useful when the caller wants explicit control over *when* cleanup
    /// happens. After this call the guard is moved and `Drop` will not run
    /// again.
    ///
    /// # Errors
    ///
    /// Returns an error if disabling raw mode or leaving the alternate screen
    /// fails.
    pub fn restore(mut self) -> io::Result<()> {
        self.needs_restore = false;
        self.cleanup()
    }

    /// Mutable reference to the inner `Terminal`.
    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }

    /// Internal cleanup shared between `Drop` and [`Self::restore`].
    ///
    /// Both operations are always attempted so a failure in the first step
    /// never prevents the second from running.
    fn cleanup(&mut self) -> io::Result<()> {
        let raw = disable_raw_mode();
        let alt = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        // Prefer the disable_raw_mode error; fall back to the alternate-screen error.
        raw.and(alt)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.needs_restore {
            // Best-effort: swallow errors in drop to avoid double-panic.
            let _ = self.cleanup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: Most TerminalGuard behaviour requires a real TTY, so we can only
    // verify structural properties here. The actual init/restore cycle is
    // covered by the TUI integration tests in `tests/tui_openai_mock.rs`.

    // ---- Compilation guarantees ----

    #[test]
    fn test_terminal_guard_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<TerminalGuard>();
    }
}
