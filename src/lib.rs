#![deny(clippy::all)]

use napi_derive::napi;

// ══════════════════════════════════════════════════════════════════════
//  Public API — napi bindings
// ══════════════════════════════════════════════════════════════════════

/// Find the PID of the process listening on the given port.
/// Searches TCP (LISTEN state) and UDP on both IPv4 and IPv6.
/// Returns null if not found.
#[napi]
pub fn find_pid_by_port(port: u16) -> Option<u32> {
    platform::find_pid_by_port(port)
}

/// Return ALL PIDs listening on the given port (TCP LISTEN + UDP, IPv4 + IPv6).
/// Returns an empty array if no process is found.
#[napi]
pub fn find_pid_by_port_all(port: u16) -> Vec<u32> {
    platform::find_pid_by_port_all(port)
}

/// Returns true if no process is listening on the given port.
#[napi]
pub fn is_port_available(port: u16) -> bool {
    platform::find_pid_by_port(port).is_none()
}

/// Poll until the given port has no LISTEN socket, or until `timeout_ms` elapses.
/// Returns true if the port became free, false on timeout.
#[napi]
pub fn wait_for_port_free(port: u16, timeout_ms: u32) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64);
    loop {
        if platform::find_pid_by_port(port).is_none() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Find all PIDs whose process name contains `name` (case-insensitive).
/// Returns an empty array if no match is found.
#[napi]
pub fn find_pids_by_name(name: String) -> Vec<u32> {
    platform::find_pids_by_name(&name)
}

/// Kill a process by PID.
/// - force=true:  SIGKILL (Unix) / TerminateProcess (Windows) — immediate
/// - force=false: SIGTERM (Unix) / taskkill /PID (Windows) — graceful
/// Returns true if the signal was sent successfully.
#[napi(ts_args_type = "pid: number, force?: boolean")]
pub fn kill(pid: u32, force: Option<bool>) -> bool {
    platform::kill(pid, force.unwrap_or(false))
}

/// Kill a process and all its children recursively (process tree).
/// Returns true if the root process was killed successfully.
#[napi(ts_args_type = "pid: number, force?: boolean")]
pub fn kill_tree(pid: u32, force: Option<bool>) -> bool {
    platform::kill_tree(pid, force.unwrap_or(false))
}

/// Find the process on `port` and kill it.
/// Returns the killed PID, or null if no process was found.
#[napi(ts_args_type = "port: number, force?: boolean")]
pub fn kill_by_port(port: u16, force: Option<bool>) -> Option<u32> {
    let pid = find_pid_by_port(port)?;
    if kill(pid, force) { Some(pid) } else { None }
}

// ══════════════════════════════════════════════════════════════════════
//  Windows — GetExtendedTcpTable / GetExtendedUdpTable (iphlpapi.dll)
// ══════════════════════════════════════════════════════════════════════

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCPROW_OWNER_PID,
        MIB_TCPTABLE_OWNER_PID, MIB_TCP_STATE_LISTEN, MIB_UDPROW_OWNER_PID,
        MIB_UDPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, TerminateProcess, PROCESS_TERMINATE,
    };

    // ── Port → PID helpers ─────────────────────────────────────────────

    /// SAFETY: `buf` must be a valid buffer returned by GetExtendedTcpTable
    /// with `TCP_TABLE_OWNER_PID_ALL` table class.
    unsafe fn tcp_pids_from_buf(buf: &[u8], port_be: u32, first_only: bool) -> Vec<u32> {
        unsafe {
            let header: &MIB_TCPTABLE_OWNER_PID = &*(buf.as_ptr().cast());
            let row_ptr = buf
                .as_ptr()
                .add(std::mem::offset_of!(MIB_TCPTABLE_OWNER_PID, table))
                as *const MIB_TCPROW_OWNER_PID;
            let rows = std::slice::from_raw_parts(row_ptr, header.dwNumEntries as usize);
            let mut result = Vec::new();
            for r in rows {
                if r.dwLocalPort == port_be && r.dwState == MIB_TCP_STATE_LISTEN as u32 {
                    result.push(r.dwOwningPid);
                    if first_only {
                        return result;
                    }
                }
            }
            result
        }
    }

    /// SAFETY: `buf` must be a valid buffer returned by GetExtendedUdpTable
    /// with `UDP_TABLE_OWNER_PID` table class.
    unsafe fn udp_pids_from_buf(buf: &[u8], port_be: u32, first_only: bool) -> Vec<u32> {
        unsafe {
            let header: &MIB_UDPTABLE_OWNER_PID = &*(buf.as_ptr().cast());
            let row_ptr = buf
                .as_ptr()
                .add(std::mem::offset_of!(MIB_UDPTABLE_OWNER_PID, table))
                as *const MIB_UDPROW_OWNER_PID;
            let rows = std::slice::from_raw_parts(row_ptr, header.dwNumEntries as usize);
            let mut result = Vec::new();
            for r in rows {
                if r.dwLocalPort == port_be {
                    result.push(r.dwOwningPid);
                    if first_only {
                        return result;
                    }
                }
            }
            result
        }
    }

    fn query_tcp(family: u32, port_be: u32, first_only: bool) -> Vec<u32> {
        unsafe {
            let mut size: u32 = 0;
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                family,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if size == 0 {
                return Vec::new();
            }
            let mut buf = vec![0u8; size as usize];
            if GetExtendedTcpTable(buf.as_mut_ptr().cast(), &mut size, 0, family, TCP_TABLE_OWNER_PID_ALL, 0) != 0
            {
                return Vec::new();
            }
            tcp_pids_from_buf(&buf, port_be, first_only)
        }
    }

    fn query_udp(family: u32, port_be: u32, first_only: bool) -> Vec<u32> {
        unsafe {
            let mut size: u32 = 0;
            GetExtendedUdpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                family,
                UDP_TABLE_OWNER_PID,
                0,
            );
            if size == 0 {
                return Vec::new();
            }
            let mut buf = vec![0u8; size as usize];
            if GetExtendedUdpTable(buf.as_mut_ptr().cast(), &mut size, 0, family, UDP_TABLE_OWNER_PID, 0)
                != 0
            {
                return Vec::new();
            }
            udp_pids_from_buf(&buf, port_be, first_only)
        }
    }

    pub fn find_pid_by_port(port: u16) -> Option<u32> {
        let port_be = port.to_be() as u32;
        let mut result = query_tcp(2, port_be, true);
        if result.is_empty() {
            result = query_tcp(23, port_be, true);
        }
        if result.is_empty() {
            result = query_udp(2, port_be, true);
        }
        if result.is_empty() {
            result = query_udp(23, port_be, true);
        }
        result.into_iter().next()
    }

    pub fn find_pid_by_port_all(port: u16) -> Vec<u32> {
        let port_be = port.to_be() as u32;
        let mut result = query_tcp(2, port_be, false);
        result.extend(query_tcp(23, port_be, false));
        result.extend(query_udp(2, port_be, false));
        result.extend(query_udp(23, port_be, false));
        result.sort_unstable();
        result.dedup();
        result
    }

    // ── Process enumeration ────────────────────────────────────────────

    fn enum_processes() -> Vec<PROCESSENTRY32> {
        let mut procs = Vec::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
                return procs;
            }
            let mut entry: PROCESSENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
            if Process32First(snap, &mut entry) != 0 {
                loop {
                    procs.push(entry);
                    if Process32Next(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
        }
        procs
    }

    pub fn find_pids_by_name(name: &str) -> Vec<u32> {
        let needle = name.to_lowercase();
        enum_processes()
            .iter()
            .filter(|e| {
                // szExeFile is [i8; 260] — convert to bytes for CStr
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                let bytes: &[u8] = unsafe { std::slice::from_raw_parts(e.szExeFile.as_ptr().cast(), len) };
                let exe = String::from_utf8_lossy(bytes);
                exe.to_lowercase().contains(&needle)
            })
            .map(|e| e.th32ProcessID)
            .collect()
    }

    // ── Kill helpers ───────────────────────────────────────────────────

    pub fn kill(pid: u32, force: bool) -> bool {
        if force {
            unsafe {
                let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
                if h.is_null() {
                    return false;
                }
                let ok = TerminateProcess(h, 1);
                CloseHandle(h);
                ok != 0
            }
        } else {
            std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
    }

    pub fn kill_tree(pid: u32, force: bool) -> bool {
        // Build parent→children map from process snapshot
        let procs = enum_processes();
        let mut children: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
        for e in &procs {
            children.entry(e.th32ParentProcessID).or_default().push(e.th32ProcessID);
        }

        // Collect all descendants (BFS)
        let mut to_kill = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(pid);
        while let Some(current) = queue.pop_front() {
            to_kill.push(current);
            if let Some(kids) = children.get(&current) {
                for &kid in kids {
                    queue.push_back(kid);
                }
            }
        }

        // Kill leaves-first (reverse order), skip the root which is last
        let mut success = true;
        for &p in to_kill.iter().rev() {
            if p == pid {
                continue; // kill root last
            }
            let _ = kill(p, force); // best-effort for children
        }
        if !kill(pid, force) {
            success = false;
        }
        success
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Linux — /proc/net parsing (zero external deps)
// ══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "linux")]
mod platform {
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    fn scan_proc_net(proto: &str, port: u16, first_only: bool) -> Vec<u32> {
        let content = match fs::read_to_string(Path::new("/proc/net").join(proto)) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let port_hex = format!("{:04X}", port);
        let is_tcp = proto.starts_with('t');
        let mut result = Vec::new();

        for line in content.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 10 {
                continue;
            }
            let local = cols[1];
            let colon = match local.rfind(':') {
                Some(c) => c,
                None => continue,
            };
            if &local[colon + 1..] != port_hex {
                continue;
            }
            if is_tcp && cols[3] != "0A" {
                continue;
            }
            if let Some(pid) = inode_to_pid(cols[9]) {
                result.push(pid);
                if first_only {
                    return result;
                }
            }
        }
        result
    }

    fn inode_to_pid(inode: &str) -> Option<u32> {
        let proc = fs::read_dir("/proc").ok()?;
        for entry in proc.flatten() {
            let name = entry.file_name();
            let pid_str = name.to_str()?;
            if !pid_str.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let fds = match fs::read_dir(format!("/proc/{}/fd", pid_str)) {
                Ok(f) => f,
                Err(_) => continue,
            };
            for fd in fds.flatten() {
                if let Ok(link) = fs::read_link(fd.path()) {
                    let s = link.to_string_lossy();
                    if s.starts_with("socket:[") && s.contains(inode) {
                        return pid_str.parse().ok();
                    }
                }
            }
        }
        None
    }

    pub fn find_pid_by_port(port: u16) -> Option<u32> {
        let mut r = scan_proc_net("tcp", port, true);
        if r.is_empty() {
            r = scan_proc_net("tcp6", port, true);
        }
        if r.is_empty() {
            r = scan_proc_net("udp", port, true);
        }
        if r.is_empty() {
            r = scan_proc_net("udp6", port, true);
        }
        r.into_iter().next()
    }

    pub fn find_pid_by_port_all(port: u16) -> Vec<u32> {
        let mut result = scan_proc_net("tcp", port, false);
        result.extend(scan_proc_net("tcp6", port, false));
        result.extend(scan_proc_net("udp", port, false));
        result.extend(scan_proc_net("udp6", port, false));
        result.sort_unstable();
        result.dedup();
        result
    }

    pub fn find_pids_by_name(name: &str) -> Vec<u32> {
        let needle = name.to_lowercase();
        let proc = match fs::read_dir("/proc") {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };
        let mut result = Vec::new();
        for entry in proc.flatten() {
            let fname = entry.file_name();
            let pid_str = match fname.to_str() {
                Some(s) => s,
                None => continue,
            };
            if !pid_str.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let comm_path = format!("/proc/{}/comm", pid_str);
            if let Ok(comm) = fs::read_to_string(&comm_path) {
                if comm.trim().to_lowercase().contains(&needle) {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        result.push(pid);
                    }
                }
            }
        }
        result
    }

    pub fn kill(pid: u32, force: bool) -> bool {
        let sig = if force { libc::SIGKILL } else { libc::SIGTERM };
        unsafe { libc::kill(pid as i32, sig) == 0 }
    }

    pub fn kill_tree(pid: u32, force: bool) -> bool {
        // Build parent→children map from /proc/*/stat
        let proc = match fs::read_dir("/proc") {
            Ok(p) => p,
            Err(_) => return false,
        };

        let mut ppid_map: HashMap<u32, u32> = HashMap::new(); // pid → ppid
        for entry in proc.flatten() {
            let fname = entry.file_name();
            let pid_str = match fname.to_str() {
                Some(s) => s,
                None => continue,
            };
            if !pid_str.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let cur_pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let stat = match fs::read_to_string(format!("/proc/{}/stat", pid_str)) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // Parse PPid from "/proc/PID/stat" — field 4 (0-indexed: 3)
            // Format: "pid (comm) state ppid ..."
            // comm can contain spaces/parens, so find the last ')' first
            let after_comm = match stat.rfind(')') {
                Some(i) => &stat[i + 2..],
                None => continue,
            };
            let fields: Vec<&str> = after_comm.split_whitespace().collect();
            if fields.len() > 2 {
                if let Ok(ppid) = fields[1].parse::<u32>() {
                    ppid_map.insert(cur_pid, ppid);
                }
            }
        }

        // Build children map
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (&child, &parent) in &ppid_map {
            children.entry(parent).or_default().push(child);
        }

        // Collect all descendants (BFS)
        let mut to_kill = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(pid);
        while let Some(current) = queue.pop_front() {
            to_kill.push(current);
            if let Some(kids) = children.get(&current) {
                for &kid in kids {
                    queue.push_back(kid);
                }
            }
        }

        // Kill leaves-first, root last
        let mut success = true;
        for &p in to_kill.iter().rev() {
            if p == pid {
                continue;
            }
            let _ = kill(p, force);
        }
        if !kill(pid, force) {
            success = false;
        }
        success
    }
}

// ══════════════════════════════════════════════════════════════════════
//  macOS — lsof + pgrep (no /proc/net)
// ══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
mod platform {
    use std::process::Command;

    pub fn find_pid_by_port(port: u16) -> Option<u32> {
        find_pid_by_port_all(port).into_iter().next()
    }

    pub fn find_pid_by_port_all(port: u16) -> Vec<u32> {
        let output = match Command::new("lsof")
            .args(["-i", &format!(":{}", port), "-sTCP:LISTEN", "-Fp"])
            .output()
        {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut result = Vec::new();
        for line in stdout.lines() {
            if let Some(pid_str) = line.strip_prefix('p') {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    result.push(pid);
                }
            }
        }
        result
    }

    pub fn find_pids_by_name(name: &str) -> Vec<u32> {
        let output = match Command::new("pgrep")
            .args(["-if", name])
            .output()
        {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .filter_map(|l| l.trim().parse::<u32>().ok())
            .collect()
    }

    pub fn kill(pid: u32, force: bool) -> bool {
        let sig = if force { libc::SIGKILL } else { libc::SIGTERM };
        unsafe { libc::kill(pid as i32, sig) == 0 }
    }

    pub fn kill_tree(pid: u32, force: bool) -> bool {
        // Use pgrep -P to find direct children, recurse
        fn find_children(parent: u32) -> Vec<u32> {
            let output = match Command::new("pgrep")
                .args(["-P", &parent.to_string()])
                .output()
            {
                Ok(o) => o,
                Err(_) => return Vec::new(),
            };
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|l| l.trim().parse::<u32>().ok())
                .collect()
        }

        fn collect_tree(root: u32, out: &mut Vec<u32>) {
            out.push(root);
            for child in find_children(root) {
                collect_tree(child, out);
            }
        }

        let mut tree = Vec::new();
        collect_tree(pid, &mut tree);

        // Kill leaves-first, root last
        let mut success = true;
        for &p in tree.iter().rev() {
            if p == pid {
                continue;
            }
            let _ = kill(p, force);
        }
        if !kill(pid, force) {
            success = false;
        }
        success
    }
}
