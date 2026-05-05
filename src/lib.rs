#![deny(clippy::all)]

use napi_derive::napi;

/// Find the PID of the process listening on the given port.
/// Searches TCP (LISTEN state) and UDP on both IPv4 and IPv6.
/// Returns null if not found.
#[napi]
pub fn find_pid_by_port(port: u16) -> Option<u32> {
    platform::find_pid_by_port(port)
}

/// Kill a process by PID.
/// - force=true:  SIGKILL (Unix) / TerminateProcess (Windows) — immediate
/// - force=false: SIGTERM (Unix) / taskkill /PID (Windows) — graceful
/// Returns true if the signal was sent successfully.
#[napi(ts_args_type = "pid: number, force?: boolean")]
pub fn kill(pid: u32, force: Option<bool>) -> bool {
    platform::kill(pid, force.unwrap_or(false))
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
    use windows_sys::Win32::System::Threading::{
        OpenProcess, TerminateProcess, PROCESS_TERMINATE,
    };

    /// SAFETY: `buf` must be a valid buffer returned by GetExtendedTcpTable
    /// with `TCP_TABLE_OWNER_PID_ALL` table class.
    unsafe fn tcp_pid_from_buf(buf: &[u8], port_be: u32) -> Option<u32> {
        unsafe {
            let header: &MIB_TCPTABLE_OWNER_PID = &*(buf.as_ptr().cast());
            let row_ptr = buf
                .as_ptr()
                .add(std::mem::offset_of!(MIB_TCPTABLE_OWNER_PID, table))
                as *const MIB_TCPROW_OWNER_PID;
            let rows = std::slice::from_raw_parts(row_ptr, header.dwNumEntries as usize);
            rows.iter().find_map(|r| {
                if r.dwLocalPort == port_be && r.dwState == MIB_TCP_STATE_LISTEN as u32 {
                    Some(r.dwOwningPid)
                } else {
                    None
                }
            })
        }
    }

    /// SAFETY: `buf` must be a valid buffer returned by GetExtendedUdpTable
    /// with `UDP_TABLE_OWNER_PID` table class.
    unsafe fn udp_pid_from_buf(buf: &[u8], port_be: u32) -> Option<u32> {
        unsafe {
            let header: &MIB_UDPTABLE_OWNER_PID = &*(buf.as_ptr().cast());
            let row_ptr = buf
                .as_ptr()
                .add(std::mem::offset_of!(MIB_UDPTABLE_OWNER_PID, table))
                as *const MIB_UDPROW_OWNER_PID;
            let rows = std::slice::from_raw_parts(row_ptr, header.dwNumEntries as usize);
            rows.iter().find_map(|r| {
                if r.dwLocalPort == port_be {
                    Some(r.dwOwningPid)
                } else {
                    None
                }
            })
        }
    }

    fn query_tcp(family: u32, port_be: u32) -> Option<u32> {
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
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            if GetExtendedTcpTable(buf.as_mut_ptr().cast(), &mut size, 0, family, TCP_TABLE_OWNER_PID_ALL, 0) != 0
            {
                return None;
            }
            tcp_pid_from_buf(&buf, port_be)
        }
    }

    fn query_udp(family: u32, port_be: u32) -> Option<u32> {
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
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            if GetExtendedUdpTable(buf.as_mut_ptr().cast(), &mut size, 0, family, UDP_TABLE_OWNER_PID, 0)
                != 0
            {
                return None;
            }
            udp_pid_from_buf(&buf, port_be)
        }
    }

    pub fn find_pid_by_port(port: u16) -> Option<u32> {
        // dwLocalPort is stored in network byte order as a u16 in the u32 field
        let port_be = port.to_be() as u32;
        query_tcp(2, port_be) // AF_INET
            .or_else(|| query_tcp(23, port_be)) // AF_INET6
            .or_else(|| query_udp(2, port_be))
            .or_else(|| query_udp(23, port_be))
    }

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
}

// ══════════════════════════════════════════════════════════════════════
//  Linux — /proc/net parsing (zero external deps)
// ══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "linux")]
mod platform {
    use std::fs;
    use std::path::Path;

    fn scan_proc_net(proto: &str, port: u16) -> Option<u32> {
        let content = fs::read_to_string(Path::new("/proc/net").join(proto)).ok()?;
        let port_hex = format!("{:04X}", port);
        let is_tcp = proto.starts_with('t');

        for line in content.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 10 {
                continue;
            }
            let local = cols[1];
            let colon = local.rfind(':')?;
            if &local[colon + 1..] != port_hex {
                continue;
            }
            if is_tcp && cols[3] != "0A" {
                continue;
            }
            return inode_to_pid(cols[9]);
        }
        None
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
        scan_proc_net("tcp", port)
            .or_else(|| scan_proc_net("tcp6", port))
            .or_else(|| scan_proc_net("udp", port))
            .or_else(|| scan_proc_net("udp6", port))
    }

    pub fn kill(pid: u32, force: bool) -> bool {
        let sig = if force { libc::SIGKILL } else { libc::SIGTERM };
        unsafe { libc::kill(pid as i32, sig) == 0 }
    }
}

// ══════════════════════════════════════════════════════════════════════
//  macOS — lsof (no /proc/net)
// ══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
mod platform {
    use std::process::Command;

    pub fn find_pid_by_port(port: u16) -> Option<u32> {
        let output = Command::new("lsof")
            .args(["-i", &format!(":{}", port), "-sTCP:LISTEN", "-Fp"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(pid_str) = line.strip_prefix('p') {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    return Some(pid);
                }
            }
        }
        None
    }

    pub fn kill(pid: u32, force: bool) -> bool {
        let sig = if force { libc::SIGKILL } else { libc::SIGTERM };
        unsafe { libc::kill(pid as i32, sig) == 0 }
    }
}
