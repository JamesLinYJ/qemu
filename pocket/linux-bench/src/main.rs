// SPDX-License-Identifier: MIT
//! Guest-only measurements. These do not include Codex/host transport latency.
mod vector_checks;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

fn distribution(mut samples: Vec<f64>) -> Value {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    let percentile = |p: usize| samples[(n * p).div_ceil(100).saturating_sub(1)];
    json!({"samples":n,"p50_ms":percentile(50),"p95_ms":percentile(95),
        "max_ms":samples[n-1],"individual_ms":samples})
}

fn process_rounds(
    program: &str,
    args: &[&str],
    rounds: usize,
    cwd: &std::path::Path,
    expected: &[u8],
) -> Result<Value> {
    let mut first = Vec::new();
    let mut total = Vec::new();
    for _ in 0..rounds {
        let start = Instant::now();
        let mut child = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stdout = child.stdout.take().context("missing stdout")?;
        let mut byte = [0];
        if stdout.read(&mut byte)? == 0 {
            let output = child.wait_with_output()?;
            bail!(
                "{program} {args:?} produced no output; status={}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        first.push(start.elapsed().as_secs_f64() * 1000.0);
        let mut rest = Vec::new();
        stdout.read_to_end(&mut rest)?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            bail!("{program}: {}", String::from_utf8_lossy(&output.stderr));
        }
        if [byte.as_slice(), rest.as_slice()].concat() != expected {
            bail!("{program} returned unexpected output");
        }
        total.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(json!({"first_byte":distribution(first),"completion":distribution(total)}))
}

struct Pty {
    process: Child,
    master: File,
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

impl Pty {
    fn open() -> Result<Self> {
        let (mut master, mut slave) = (-1, -1);
        if unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        let mut term = std::mem::MaybeUninit::<libc::termios>::uninit();
        if unsafe { libc::tcgetattr(slave.as_raw_fd(), term.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut term = unsafe { term.assume_init() };
        term.c_lflag &= !(libc::ECHO | libc::ECHONL);
        if unsafe { libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &term) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut command = Command::new("/bin/bash");
        command
            .args(["--noprofile", "--norc", "-i"])
            .env("PS1", "__POCKET_PROMPT__")
            .env("PS2", "")
            .env("TERM", "dumb")
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave));
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let process = command.spawn()?;
        Ok(Self {
            process,
            master: File::from(master),
        })
    }

    fn until_prompt(&mut self, expected: Option<&str>, start: Instant) -> Result<f64> {
        let mut output = Vec::new();
        let mut first = None;
        while start.elapsed() < Duration::from_secs(30) {
            let mut poll = libc::pollfd {
                fd: self.master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut poll, 1, 100) };
            if ready < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.into());
            }
            if ready == 0 {
                continue;
            }
            let mut bytes = [0; 4096];
            let n = self.master.read(&mut bytes)?;
            if n == 0 {
                bail!("PTY closed before command completed");
            }
            first.get_or_insert_with(|| start.elapsed().as_secs_f64() * 1000.0);
            output.extend_from_slice(&bytes[..n]);
            let text = String::from_utf8_lossy(&output);
            if text.contains("__POCKET_PROMPT__")
                && expected.is_none_or(|value| text.contains(value))
            {
                return first.context("missing PTY output timestamp");
            }
            if output.len() > 16384 {
                bail!("unexpected PTY output volume");
            }
        }
        bail!("PTY command timed out")
    }
}

fn pty_rounds() -> Result<Value> {
    let start = Instant::now();
    let mut pty = Pty::open()?;
    pty.until_prompt(None, start)?;
    let startup = start.elapsed().as_secs_f64() * 1000.0;
    let mut first = Vec::new();
    let mut total = Vec::new();
    for i in 0..50 {
        let marker = format!("__POCKET_RESULT_{i}__");
        let start = Instant::now();
        writeln!(pty.master, "printf '{marker}\\n'")?;
        pty.master.flush()?;
        first.push(pty.until_prompt(Some(&marker), start)?);
        total.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    pty.master.write_all(b"exit\n")?;
    if !pty.process.wait()?.success() {
        bail!("interactive Bash exited unsuccessfully");
    }
    Ok(
        json!({"startup_ms":startup,"first_byte":distribution(first),"completion":distribution(total)}),
    )
}

fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--check-vectors") {
        return vector_checks::run();
    }
    if distribution(vec![3.0, 1.0, 2.0])["p95_ms"] != 3.0 {
        bail!("invalid statistics");
    }
    let cwd = std::env::temp_dir().join(format!("pocket-shell-bench-{}", std::process::id()));
    fs::create_dir(&cwd)?;
    let result = (|| -> Result<Value> {
        fs::write(cwd.join("sample.txt"), "pocket needle\n")?;
        let mut result = serde_json::Map::new();
        for (name, program, args, count, expected) in [
            (
                "bash_login",
                "/bin/bash",
                vec!["-lc", "printf pocket"],
                50,
                "pocket",
            ),
            (
                "bash_no_profile",
                "/bin/bash",
                vec!["--noprofile", "--norc", "-c", "printf pocket"],
                50,
                "pocket",
            ),
            (
                "bash_pipeline",
                "/bin/bash",
                vec!["-lc", "printf 'one\\ntwo\\n' | grep two | wc -l"],
                20,
                "1\n",
            ),
            (
                "shell_rg",
                "/bin/bash",
                vec!["-lc", "rg needle sample.txt"],
                20,
                "pocket needle\n",
            ),
            (
                "python_start",
                "/usr/bin/python3",
                vec!["-c", "print('pocket')"],
                10,
                "pocket\n",
            ),
        ] {
            eprintln!("POCKET_SHELL_CASE:{name}");
            let measured = process_rounds(program, &args, count, &cwd, expected.as_bytes())?;
            eprintln!("POCKET_SHELL_MEASUREMENT:{name}:{measured}");
            result.insert(name.into(), measured);
        }
        result.insert("persistent_pty".into(), pty_rounds()?);
        Ok(
            json!({"scope":"guest Linux only; excludes Codex and host-to-guest transport",
            "results":result}),
        )
    })();
    fs::remove_dir_all(&cwd)?;
    println!("POCKET_SHELL_REPORT:{}", serde_json::to_string(&result?)?);
    Ok(())
}
