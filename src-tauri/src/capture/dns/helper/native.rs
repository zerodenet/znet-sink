use std::io;

#[cfg(target_os = "linux")]
pub(super) struct Owner {
    pid: u32,
    start: String,
}
#[cfg(target_os = "linux")]
impl Owner {
    fn start(pid: u32) -> io::Result<String> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
        // comm is parenthesized and may contain spaces; starttime is field 22.
        stat.rsplit_once(')')
            .and_then(|(_, tail)| tail.split_whitespace().nth(19))
            .map(str::to_owned)
            .ok_or_else(|| io::Error::other("invalid process identity"))
    }
    pub fn new(pid: u32) -> io::Result<Self> {
        Ok(Self {
            pid,
            start: Self::start(pid)?,
        })
    }
    pub fn alive(&self) -> bool {
        Self::start(self.pid).is_ok_and(|start| start == self.start)
    }
}
#[cfg(target_os = "windows")]
pub(super) struct Owner(windows_sys::Win32::Foundation::HANDLE);
#[cfg(target_os = "windows")]
impl Owner {
    pub fn new(pid: u32) -> io::Result<Self> {
        let handle =
            unsafe { windows_sys::Win32::System::Threading::OpenProcess(0x00100000, 0, pid) };
        if handle.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }
    pub fn alive(&self) -> bool {
        unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(self.0, 0) == 258 }
    }
}
#[cfg(target_os = "windows")]
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

pub(super) struct Resolver {
    #[cfg(target_os = "linux")]
    interface: String,
    server: String,
    #[cfg(windows)]
    name: String,
    removed: bool,
}

fn execute(program: &str, args: &[&str]) -> io::Result<String> {
    let output = crate::services::common::background_command(program)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
#[cfg(windows)]
fn powershell(script: &str) -> io::Result<String> {
    execute(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("$ErrorActionPreference='Stop'; {script}"),
        ],
    )
}

#[cfg(target_os = "linux")]
fn link_values(output: &str) -> Vec<&str> {
    output
        .split_once(':')
        .map(|(_, values)| values.split_whitespace().collect())
        .unwrap_or_default()
}

impl Resolver {
    pub fn install(interface: &str, server: &str, _pid: u32) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            // Require the host to actually use resolved, not merely have its CLI.
            let config = std::fs::read_to_string("/etc/resolv.conf")?;
            if !config.lines().any(|line| {
                line.split_whitespace()
                    .take(2)
                    .eq(["nameserver", "127.0.0.53"])
            }) {
                return Err(io::Error::other("系统 DNS 未使用 systemd-resolved stub；请配置解析入口后重试，未修改 resolv.conf"));
            }
            execute("resolvectl", &["status", interface])?;
            // This client owns the TUN link, but must not overwrite foreign
            // per-link DNS configuration if another manager already set it.
            for property in ["dns", "domain"] {
                let current = execute("resolvectl", &[property, interface])?;
                if !link_values(&current).is_empty() {
                    return Err(io::Error::other(
                        "TUN 接口已有 DNS 配置，未覆盖其他管理器的设置",
                    ));
                }
            }
            let mut guard = Self {
                interface: interface.into(),
                server: server.into(),
                removed: false,
            };
            let result = (|| {
                execute("resolvectl", &["dns", interface, server])?;
                execute("resolvectl", &["domain", interface, "~."])?;
                Ok(())
            })();
            if let Err(error) = result {
                let _ = guard.remove();
                return Err(error);
            }
            Ok(guard)
        }
        #[cfg(windows)]
        {
            // The rule belongs to this guardian, not the kernel. If cleanup
            // fails and the guardian exits, a retry can recognize its orphan.
            let guardian_pid = std::process::id();
            let script = format!(
                r#"
$prefix='ZNet Sink TUN DNS v1';
$existing=@(Get-DnsClientNrptRule | Where-Object {{ $_.Comment -like "$prefix;*" }});
foreach ($rule in $existing) {{
 if (@($rule.Namespace).Count -ne 1 -or $rule.Namespace[0] -ne '.' -or $rule.DisplayName -ne 'ZNet Sink TUN DNS') {{ throw 'managed DNS rule was changed externally; inspect it before retrying' }}
 if ($rule.Comment -match '^ZNet Sink TUN DNS v1;pid=(\d+);start=(\d+);') {{
  $owner=Get-Process -Id ([int]$Matches[1]) -ErrorAction SilentlyContinue;
  if ($owner -and $owner.StartTime.ToUniversalTime().Ticks -eq ([long]$Matches[2])) {{ throw 'another TUN DNS guardian is active' }}
  Remove-DnsClientNrptRule -Name $rule.Name -Force;
 }} else {{ throw 'unrecognized managed DNS rule; inspect it before retrying' }}
}}
$foreign=@(Get-DnsClientNrptRule | Where-Object {{ @($_.Namespace) -contains '.' }});
$effective=@(Get-DnsClientNrptPolicy -Effective | Where-Object {{ @($_.Namespace) -contains '.' }});
if ($foreign.Count -gt 0 -or $effective.Count -gt 0) {{ throw '已有全局 DNS 策略，未覆盖其他 VPN 或组织策略' }}
$start=(Get-Process -Id {guardian_pid}).StartTime.ToUniversalTime().Ticks;
$comment="$prefix;pid={guardian_pid};start=$start;interface={interface}";
$rule=$null;
try {{
 $rule=Add-DnsClientNrptRule -Namespace '.' -NameServers '{server}' -Comment $comment -DisplayName 'ZNet Sink TUN DNS' -PassThru;
 Clear-DnsClientCache;
 $rule.Name
}} catch {{
 if ($rule) {{ Remove-DnsClientNrptRule -Name $rule.Name -Force }}
 throw
}}
"#
            );
            let name = powershell(&script)?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() || b"-{}".contains(&c))
            {
                return Err(io::Error::other("DNS rule identity was not returned"));
            }
            Ok(Self {
                server: server.into(),
                name,
                removed: false,
            })
        }
    }
    pub fn remove(&mut self) -> io::Result<()> {
        if self.removed {
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        {
            // A vanished TUN automatically loses its per-link settings.
            if !std::path::Path::new("/sys/class/net")
                .join(&self.interface)
                .exists()
            {
                self.removed = true;
                return Ok(());
            }
            let dns = execute("resolvectl", &["dns", &self.interface])?;
            let domain = execute("resolvectl", &["domain", &self.interface])?;
            let servers = link_values(&dns);
            let domains = link_values(&domain);
            if (servers.is_empty() || servers == [self.server.as_str()])
                && (domains.is_empty() || domains == ["~."])
            {
                // Clear only the two properties installed by this guardian.
                // Leave the link's other resolved settings unchanged.
                execute("resolvectl", &["dns", &self.interface, ""])?;
                execute("resolvectl", &["domain", &self.interface, ""])?;
            } else if servers.contains(&self.server.as_str()) {
                return Err(io::Error::other(
                    "TUN DNS 配置被外部修改，未撤销其他设置；请检查此接口",
                ));
            }
        }
        #[cfg(windows)]
        {
            powershell(&format!(
                r#"$rule=Get-DnsClientNrptRule -Name '{}' -ErrorAction SilentlyContinue; if ($rule -and $rule.Comment -like 'ZNet Sink TUN DNS v1;*' -and @($rule.NameServers).Count -eq 1 -and $rule.NameServers[0] -eq '{}' -and @($rule.Namespace).Count -eq 1 -and $rule.Namespace[0] -eq '.') {{ Remove-DnsClientNrptRule -Name $rule.Name -Force; Clear-DnsClientCache }}"#,
                self.name, self.server
            ))?;
        }
        self.removed = true;
        Ok(())
    }
}
impl Drop for Resolver {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}
