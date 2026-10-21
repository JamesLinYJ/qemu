// SPDX-License-Identifier: MIT
//! Native in-process QEMU validation host. One QEMU lifecycle per process.
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    ffi::CString,
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

mod native {
    include!(concat!(env!("OUT_DIR"), "/qemu.rs"));
}

mod mach {
    include!(concat!(env!("OUT_DIR"), "/memory.rs"));
}

fn physical_memory() -> Option<Value> {
    let mut info = std::mem::MaybeUninit::<mach::TaskVmInfo>::zeroed();
    let mut count = (std::mem::size_of::<mach::TaskVmInfo>() / std::mem::size_of::<i32>()) as u32;
    let result = unsafe {
        mach::task_info(
            mach::mach_task_self_,
            mach::TASK_VM_INFO as _,
            info.as_mut_ptr().cast(),
            &mut count,
        )
    };
    if result != 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let available = count as usize * std::mem::size_of::<i32>();
    let footprint = (available >= std::mem::offset_of!(mach::TaskVmInfo, phys_footprint) + 8)
        .then_some(info.phys_footprint);
    let peak = (available
        >= std::mem::offset_of!(mach::TaskVmInfo, ledger_phys_footprint_peak) + 8)
        .then_some(info.ledger_phys_footprint_peak);
    Some(json!({"physical_footprint_bytes":footprint,
        "peak_physical_footprint_bytes":peak,"returned_mach_words":count}))
}

static STARTED: AtomicBool = AtomicBool::new(false);

// Read each console byte once. Keep a short suffix so markers split between
// writes are recognized without repeatedly reading an ever-growing log.
#[derive(Default)]
struct ConsoleProgress {
    suffix: Vec<u8>,
    bytes_read: u64,
    passed: bool,
    failed: bool,
}

impl ConsoleProgress {
    fn read(&mut self, input: &mut impl Read) -> std::io::Result<()> {
        const PASS: &[u8] = b"POCKET_ACCEPTANCE_PASS";
        const FAIL: &[u8] = b"POCKET_ACCEPTANCE_FAIL";
        const PANIC: &[u8] = b"Kernel panic";
        let mut buffer = [0; 8192];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                return Ok(());
            }
            self.bytes_read += count as u64;
            self.suffix.extend_from_slice(&buffer[..count]);
            let contains = |marker: &[u8]| self.suffix.windows(marker.len()).any(|v| v == marker);
            self.passed |= contains(PASS);
            self.failed |= contains(FAIL) || contains(PANIC);
            let retain = self.suffix.len().saturating_sub(PASS.len() - 1);
            self.suffix.drain(..retain);
        }
    }
}

struct Qmp {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
    events: Vec<Value>,
}

impl Qmp {
    fn connect(path: &Path, deadline: Instant) -> Result<Self> {
        let stream = loop {
            match UnixStream::connect(path) {
                Ok(stream) => break stream,
                Err(error) if Instant::now() < deadline => {
                    if error.kind() != std::io::ErrorKind::NotFound
                        && error.kind() != std::io::ErrorKind::ConnectionRefused
                    {
                        return Err(error.into());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error).context("QMP startup timed out"),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        let mut qmp = Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: stream,
            next_id: 0,
            events: Vec::new(),
        };
        let greeting = qmp.read()?;
        if greeting.get("QMP").is_none() {
            bail!("invalid QMP greeting: {greeting}");
        }
        qmp.execute("qmp_capabilities", json!({}))?;
        Ok(qmp)
    }

    fn read(&mut self) -> Result<Value> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            bail!("QMP closed");
        }
        serde_json::from_str(&line).context("invalid QMP JSON")
    }

    fn execute(&mut self, command: &str, arguments: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        serde_json::to_writer(
            &mut self.writer,
            &json!({"execute":command,"arguments":arguments,"id":id}),
        )?;
        self.writer.write_all(b"\r\n")?;
        loop {
            let response = self.read()?;
            if response.get("event").is_some() {
                self.events.push(response);
                continue;
            }
            if response.get("id").and_then(Value::as_u64) != Some(id) {
                bail!("unexpected QMP response id: {response}");
            }
            if let Some(error) = response.get("error") {
                bail!("QMP {command}: {error}");
            }
            return response
                .get("return")
                .cloned()
                .context("missing QMP return");
        }
    }
}

fn start_native(arguments: Vec<String>) -> Result<thread::JoinHandle<i32>> {
    if STARTED.swap(true, Ordering::AcqRel) {
        bail!("QEMU can only start once per process");
    }
    let arguments: Vec<CString> = arguments
        .into_iter()
        .map(CString::new)
        .collect::<std::result::Result<_, _>>()?;
    // Arguments remain alive through cleanup. QEMU owns its global state and runs
    // init, the locked main loop and cleanup on the same worker thread.
    Ok(thread::Builder::new()
        .name("pocket-qemu".into())
        .spawn(move || {
            let mut argv: Vec<_> = arguments
                .iter()
                .map(|arg| arg.as_ptr().cast_mut())
                .collect();
            argv.push(std::ptr::null_mut());
            unsafe {
                native::qemu_init(arguments.len() as i32, argv.as_mut_ptr());
                let status = native::qemu_main_loop();
                native::qemu_cleanup(status);
                native::bql_unlock();
                native::replay_mutex_unlock();
                status
            }
        })?)
}

fn wait_for_job(qmp: &mut Qmp, id: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let jobs = qmp.execute("query-jobs", json!({}))?;
        let job = jobs
            .as_array()
            .context("invalid jobs response")?
            .iter()
            .find(|job| job["id"] == id)
            .context("job disappeared")?;
        if job["status"] == "concluded" {
            let error = job.get("error").cloned();
            qmp.execute("job-dismiss", json!({"id":id}))?;
            if let Some(error) = error {
                bail!("job {id}: {error}");
            }
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("job {id} timed out");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn disk_checks(qmp: &mut Qmp, state: &Path) -> Result<Value> {
    let disk = state.join("snapshot-test.qcow2");
    fs::File::create_new(&disk)?;
    qmp.execute(
        "blockdev-add",
        json!({"driver":"file","node-name":"test-file","filename":disk}),
    )?;
    qmp.execute(
        "blockdev-create",
        json!({"job-id":"create-test-disk","options":{
        "driver":"qcow2","file":"test-file","size":67108864}}),
    )?;
    wait_for_job(qmp, "create-test-disk")?;
    qmp.execute(
        "blockdev-add",
        json!({"driver":"qcow2","node-name":"test-root","file":"test-file"}),
    )?;
    qmp.execute(
        "device_add",
        // The test VM is paused. Explicit sharing allows the NBD verifier to
        // exercise the same disk frontend without concurrent guest writes.
        json!({"driver":"virtio-blk-pci","drive":"test-root","id":"test-disk","bus":"disk-port","share-rw":true}),
    )?;
    let socket = state.join("nbd.sock");
    qmp.execute(
        "nbd-server-start",
        json!({"addr":{"type":"unix","data":{"path":socket}}}),
    )?;
    let export = |qmp: &mut Qmp| -> Result<()> {
        qmp.execute(
            "block-export-add",
            json!({"type":"nbd","id":"test-export",
            "node-name":"test-root","name":"disk","writable":true}),
        )?;
        Ok(())
    };
    let unexport = |qmp: &mut Qmp| -> Result<()> {
        qmp.execute(
            "block-export-del",
            json!({"id":"test-export","mode":"hard"}),
        )?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let exports = qmp.execute("query-block-exports", json!({}))?;
            if exports
                .as_array()
                .context("invalid exports response")?
                .is_empty()
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                bail!("NBD export cleanup timed out");
            }
            thread::sleep(Duration::from_millis(10));
        }
    };
    // macOS validation helper only: it sends actual block I/O to our embedded
    // QEMU's NBD export and returns failure when pattern verification fails.
    let io = |command: &str| -> Result<String> {
        let output = std::process::Command::new("qemu-io")
            .args(["-f", "raw", "-c", command])
            .arg(format!("nbd+unix:///disk?socket={}", socket.display()))
            .output()?;
        if !output.status.success() {
            bail!(
                "block check {command}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(String::from_utf8(output.stdout)?)
    };
    let monitor = |qmp: &mut Qmp, command: &str| -> Result<()> {
        let output = qmp.execute("human-monitor-command", json!({"command-line":command}))?;
        if output.as_str() != Some("") {
            bail!("{command}: {output}");
        }
        Ok(())
    };
    export(qmp)?;
    let write_a = io("write -P 0x41 0 4096")?;
    io("read -P 0x41 0 4096")?;
    unexport(qmp)?;
    monitor(qmp, "savevm pocket-test")?;
    export(qmp)?;
    let write_b = io("write -P 0x42 0 4096")?;
    io("read -P 0x42 0 4096")?;
    unexport(qmp)?;
    monitor(qmp, "loadvm pocket-test")?;
    export(qmp)?;
    let restored_a = io("read -P 0x41 0 4096")?;
    unexport(qmp)?;
    let network = qmp.execute(
        "human-monitor-command",
        json!({"command-line":"info usernet"}),
    )?;
    Ok(
        json!({"qcow2_created_by_embedded_qemu":true,"write_a":write_a,"write_b":write_b,
        "restored_a":restored_a,"snapshot_restore_verified":true,"user_network_backend":network,
        "guest_network_traffic_tested":false}),
    )
}

fn main() -> Result<()> {
    let timeout_secs = match std::env::var("POCKET_VM_TIMEOUT_SECS") {
        Ok(value) => value
            .parse::<u64>()
            .context("invalid POCKET_VM_TIMEOUT_SECS")?,
        Err(std::env::VarError::NotPresent) => 600,
        Err(error) => return Err(error).context("invalid POCKET_VM_TIMEOUT_SECS"),
    };
    let cpu = std::env::var("POCKET_VM_CPU").unwrap_or_else(|_| "cortex-a72".into());
    if !matches!(
        cpu.as_str(),
        "cortex-a72" | "cortex-a76" | "neoverse-n1" | "max"
    ) {
        bail!("unsupported validation CPU model");
    }
    let cpu_count = std::env::var("POCKET_VM_CPUS")
        .unwrap_or_else(|_| "1".into())
        .parse::<u8>()
        .context("invalid POCKET_VM_CPUS")?;
    if !(1..=4).contains(&cpu_count) {
        bail!("POCKET_VM_CPUS must be 1 to 4");
    }
    if env!("POCKET_INTERPRETER") == "tci" && cpu_count != 1 {
        bail!("The TCI reference profile uses one CPU");
    }
    let accelerator = if cpu_count > 1 {
        "tcg,thread=multi"
    } else {
        "tcg,thread=single"
    };
    let tb_mib = match std::env::var("POCKET_VM_TB_MIB") {
        Ok(value) => Some(value.parse::<u32>().context("invalid POCKET_VM_TB_MIB")?),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(error).context("invalid POCKET_VM_TB_MIB"),
    };
    if tb_mib.is_some_and(|size| !(32..=1024).contains(&size)) {
        bail!("POCKET_VM_TB_MIB must be 32 to 1024");
    }
    let accelerator = match tb_mib {
        Some(size) => format!("{accelerator},tb-size={size}"),
        None => accelerator.to_owned(),
    };
    let mut cli = std::env::args().skip(1);
    let first = cli
        .next()
        .context("usage: pocket-vm [--resources] STATE_DIR [KERNEL [ROOT_DISK]]")?;
    let resources = first == "--resources";
    let state = PathBuf::from(if resources {
        cli.next().context("missing STATE_DIR")?
    } else {
        first
    });
    let kernel = cli.next().map(PathBuf::from);
    let disk = cli.next().map(PathBuf::from);
    if cli.next().is_some() {
        bail!("too many arguments");
    }
    // An existing directory may contain another live VM's sockets or evidence.
    fs::create_dir(&state).context("STATE_DIR must not already exist")?;
    let state = state.canonicalize()?;
    let socket = state.join("qmp.sock");
    let console = state.join("console.log");
    let start = Instant::now();
    let mut args: Vec<String> = [
        "pocket-agent",
        "-machine",
        "virt",
        "-cpu",
        &cpu,
        "-accel",
        &accelerator,
        "-m",
        "512",
        "-smp",
        &cpu_count.to_string(),
        "-nodefaults",
        "-display",
        "none",
        "-monitor",
        "none",
        "-S",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    args.extend([
        "-qmp".into(),
        format!("unix:{},server=on,wait=off", socket.display()),
        "-serial".into(),
        format!("file:{}", console.display()),
        "-netdev".into(),
        "user,id=net0".into(),
        "-device".into(),
        "virtio-net-device,netdev=net0".into(),
    ]);
    if resources {
        args.extend([
            "-device".into(),
            "pcie-root-port,id=disk-port,chassis=1".into(),
        ]);
    }
    if let Some(kernel) = &kernel {
        args.extend([
            "-kernel".into(),
            kernel.canonicalize()?.to_string_lossy().into_owned(),
            "-append".into(),
            "console=ttyAMA0 root=/dev/vda rw rootwait init=/sbin/init panic=-1".into(),
        ]);
    }
    if let Some(disk) = disk {
        args.extend([
            "-drive".into(),
            format!(
                "if=none,id=root,format=raw,file={}",
                disk.canonicalize()?.display()
            ),
            "-device".into(),
            "virtio-blk-device,drive=root".into(),
            "-object".into(),
            "rng-random,id=rng0,filename=/dev/urandom".into(),
            "-device".into(),
            "virtio-rng-device,rng=rng0".into(),
        ]);
    }
    fs::write(
        state.join("arguments.json"),
        serde_json::to_vec_pretty(&args)?,
    )?;
    let worker = start_native(args)?;
    let mut qmp = Qmp::connect(&socket, start + Duration::from_secs(30))?;
    let ready_ms = start.elapsed().as_secs_f64() * 1000.0;
    let version = qmp.execute("query-version", json!({}))?;
    let initial = qmp.execute("query-status", json!({}))?;
    let resource_checks = if resources {
        Some(disk_checks(&mut qmp, &state)?)
    } else {
        None
    };
    if !resources {
        qmp.execute("cont", json!({}))?;
    }
    let running = qmp.execute("query-status", json!({}))?;
    let mut guest_passed = false;
    let mut console_progress = ConsoleProgress::default();
    let mut translation_stats = None;
    let mut block_stats = None;
    let mut workload_ms = None;
    if kernel.is_some() {
        let mut console_file = fs::File::open(&console).context("open QEMU console")?;
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        while !worker.is_finished() && Instant::now() < deadline {
            console_progress.read(&mut console_file)?;
            if console_progress.failed {
                break;
            }
            if console_progress.passed {
                guest_passed = true;
                workload_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
                // QEMU calls this x-query-jit even when the selected TCG backend
                // is an interpreter. It exposes TB/TLB statistics, not JIT use.
                translation_stats = Some(qmp.execute("x-query-jit", json!({}))?);
                block_stats = Some(qmp.execute("query-blockstats", json!({}))?);
                let shutdown_deadline = Instant::now() + Duration::from_secs(15);
                while !worker.is_finished() && Instant::now() < shutdown_deadline {
                    thread::sleep(Duration::from_millis(100));
                }
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    if !worker.is_finished() {
        qmp.execute("quit", json!({}))?;
    }
    let status = worker
        .join()
        .map_err(|_| anyhow::anyhow!("QEMU worker panicked"))?;
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // getrusage initializes the supplied record on success; macOS reports RSS in bytes.
    let resource_usage = unsafe {
        if libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) == 0 {
            let usage = usage.assume_init();
            let seconds = |time: libc::timeval| time.tv_sec as f64 + time.tv_usec as f64 / 1e6;
            Some(json!({"peak_rss_bytes":usage.ru_maxrss,
                "user_cpu_seconds":seconds(usage.ru_utime),
                "system_cpu_seconds":seconds(usage.ru_stime),
                "voluntary_context_switches":usage.ru_nvcsw,
                "involuntary_context_switches":usage.ru_nivcsw}))
        } else {
            None
        }
    };
    let report = json!({"host":std::env::consts::OS,"architecture":std::env::consts::ARCH,
        "embedding":"native-shared-library","jit":false,"hvf":false,"version":version,
        "interpreter":env!("POCKET_INTERPRETER"),"resource_usage":resource_usage,
        "host_memory":physical_memory(),
        "cpu_model":cpu,"cpu_count":cpu_count,"accelerator":accelerator,
        "tb_cache_mib":tb_mib,"translation_stats":translation_stats,"block_stats":block_stats,
        "workload_ms":workload_ms,"console_bytes_read":console_progress.bytes_read,
        "qmp_ready_ms":ready_ms,"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
        "initial":initial,"running":running,"status":status,"events":qmp.events,
        "resource_checks":resource_checks,
        "kernel_test_requested":kernel.is_some(),"guest_acceptance_passed":guest_passed});
    fs::write(
        state.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if status != 0 || (kernel.is_some() && !guest_passed) {
        bail!("VM acceptance did not pass");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ConsoleProgress;

    #[test]
    fn console_markers_survive_fragmented_binary_output() {
        let mut progress = ConsoleProgress::default();
        progress
            .read(&mut b"\xfflog\nPOCKET_ACCEPTANCE_PA".as_slice())
            .unwrap();
        assert!(!progress.passed);
        progress.read(&mut b"SS\r\n".as_slice()).unwrap();
        assert!(progress.passed);
        progress
            .read(&mut b"POCKET_ACCEPTANCE_FA".as_slice())
            .unwrap();
        progress.read(&mut b"IL:1\n".as_slice()).unwrap();
        assert!(progress.failed);
        assert!(progress.suffix.len() < b"POCKET_ACCEPTANCE_PASS".len());
    }
}
