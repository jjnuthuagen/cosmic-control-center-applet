//! Launching other programs without leaving zombies behind.
//!
//! A panel applet runs for the whole session. `Command::spawn` on its own
//! leaves every child in the process table as a zombie once it exits, because
//! nothing ever reaps it — the applet is the parent and it never calls `wait`.
//! Opening Settings a dozen times, or pressing a custom tile on a timer, would
//! accumulate entries that only clear when the applet itself exits.
//!
//! Reaping on a detached thread is the smallest fix that keeps the launch
//! itself non-blocking: the thread parks in `waitpid` for as long as the child
//! lives, then exits. Custom tiles are user commands that may run for hours, so
//! nothing about that wait is on a path the UI cares about.

use std::process::Command;

/// Spawn `command`, reaping it in the background.
///
/// Returns the child's pid on success. The child's exit status is deliberately
/// discarded: these are fire-and-forget launches, and there is nowhere in a
/// panel popup to report one.
pub fn spawn_and_reap(mut command: Command) -> std::io::Result<u32> {
    let mut child = command.spawn()?;
    let pid = child.id();
    std::thread::Builder::new()
        .name(format!("reap-{pid}"))
        .spawn(move || {
            if let Err(err) = child.wait() {
                tracing::debug!("could not reap pid {pid}: {err}");
            }
        })
        // A machine that cannot start a thread has larger problems, and the
        // child is already running — losing the reaper is not worth failing the
        // launch over.
        .map_err(|err| tracing::warn!("could not start a reaper for pid {pid}: {err}"))
        .ok();
    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_is_reaped_rather_than_left_as_a_zombie() {
        let mut command = Command::new("true");
        command.stdout(std::process::Stdio::null());
        let pid = spawn_and_reap(command).expect("`true` must be runnable");

        // Reading /proc is the only way to see the distinction this function
        // exists for: an unreaped child stays listed, in state Z, forever.
        //
        // A *correctly* reaped child is also briefly Z — between its exit and
        // the reaper's `wait` — so seeing Z once proves nothing. The test is
        // whether it goes away, which is why this waits for disappearance
        // rather than asserting on the first sample. Asserting on the first Z
        // is what made an earlier version of this test fail on a loaded CI
        // runner and pass everywhere else.
        let path = format!("/proc/{pid}/stat");
        let mut last_state = None;
        for _ in 0..500 {
            let Ok(stat) = std::fs::read_to_string(&path) else {
                // Gone entirely: reaped.
                return;
            };
            // `comm` can contain spaces and parentheses, so the state field is
            // the first token after the last ')'.
            last_state = stat
                .rsplit_once(')')
                .and_then(|(_, rest)| rest.split_whitespace().next())
                .map(str::to_string);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("pid {pid} never went away; it is still in state {last_state:?}");
    }
}
