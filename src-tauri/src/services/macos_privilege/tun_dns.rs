//! Per-session resolver. No persistent network-service preferences are changed.
use std::{
    ffi::{c_void, CString},
    io, ptr,
};

type Ref = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(allocator: Ref, value: *const i8, encoding: u32) -> Ref;
    fn CFDataCreate(allocator: Ref, bytes: *const u8, length: isize) -> Ref;
    fn CFPropertyListCreateWithData(
        allocator: Ref,
        data: Ref,
        options: usize,
        format: *mut isize,
        error: *mut Ref,
    ) -> Ref;
    fn CFRelease(value: Ref);
    fn CFEqual(left: Ref, right: Ref) -> u8;
    fn CFDictionaryCreateMutable(
        allocator: Ref,
        capacity: isize,
        key_callbacks: Ref,
        value_callbacks: Ref,
    ) -> Ref;
    fn CFDictionarySetValue(dict: Ref, key: Ref, value: Ref);
    static kCFBooleanTrue: Ref;
}
#[link(name = "SystemConfiguration", kind = "framework")]
unsafe extern "C" {
    fn SCDynamicStoreCreateWithOptions(
        allocator: Ref,
        name: Ref,
        options: Ref,
        callback: Ref,
        context: Ref,
    ) -> Ref;
    fn SCDynamicStoreSetValue(store: Ref, key: Ref, value: Ref) -> u8;
    fn SCDynamicStoreCopyValue(store: Ref, key: Ref) -> Ref;
    fn SCError() -> i32;
    static kSCDynamicStoreUseSessionKeys: Ref;
}

struct Owned(Ref);
impl Owned {
    fn checked(value: Ref) -> io::Result<Self> {
        if value.is_null() {
            Err(io::Error::other("SystemConfiguration allocation failed"))
        } else {
            Ok(Self(value))
        }
    }
    fn string(value: &str) -> io::Result<Self> {
        let value = CString::new(value).map_err(io::Error::other)?;
        // SAFETY: CF copies the live, NUL-terminated UTF-8 input.
        Self::checked(unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), 0x08000100) })
    }
    fn plist(xml: &str) -> io::Result<Self> {
        // SAFETY: CFData copies this byte slice; the plist parser borrows it.
        let data =
            Self::checked(unsafe { CFDataCreate(ptr::null(), xml.as_ptr(), xml.len() as isize) })?;
        Self::checked(unsafe {
            CFPropertyListCreateWithData(ptr::null(), data.0, 0, ptr::null_mut(), ptr::null_mut())
        })
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

pub(super) struct Resolver {
    // Releasing the session (including helper death) removes its session keys.
    store: Owned,
    key: Owned,
    value: Owned,
    pub interface: String,
    pub pid: u32,
    pub server: String,
}
impl Resolver {
    pub fn install(interface: &str, server: &str, pid: u32) -> io::Result<Self> {
        crate::capture::dns::validate_target(interface, server, pid)?;
        if !interface.starts_with("utun") || !Self::alive(interface, pid) {
            return Err(io::Error::other(
                "TUN DNS owner or interface is unavailable",
            ));
        }
        let name = Owned::string("ZNet Sink TUN DNS")?;
        let options = Owned::checked(unsafe {
            CFDictionaryCreateMutable(ptr::null(), 1, ptr::null(), ptr::null())
        })?;
        unsafe { CFDictionarySetValue(options.0, kSCDynamicStoreUseSessionKeys, kCFBooleanTrue) };
        let store = Owned::checked(unsafe {
            SCDynamicStoreCreateWithOptions(
                ptr::null(),
                name.0,
                options.0,
                ptr::null(),
                ptr::null(),
            )
        })?;
        let key = Owned::string(&format!(
            "State:/Network/Service/org.zerodenet.znetsink-{interface}/DNS"
        ))?;
        let value = Owned::plist(&format!(
            "<plist version=\"1.0\"><dict><key>ServerAddresses</key><array><string>{server}</string></array><key>SupplementalMatchDomains</key><array><string></string></array><key>SupplementalMatchOrders</key><array><integer>1</integer></array><key>SupplementalMatchDomainsNoSearch</key><integer>1</integer><key>SearchOrder</key><integer>1</integer></dict></plist>"
        ))?;
        let resolver = Self {
            store,
            key,
            value,
            interface: interface.into(),
            pid,
            server: server.into(),
        };
        resolver.refresh()?;
        Ok(resolver)
    }
    pub fn refresh(&self) -> io::Result<()> {
        // Avoid sending repeated DNS configuration notifications for an
        // unchanged value. A missing private session key is re-published.
        let current = unsafe { SCDynamicStoreCopyValue(self.store.0, self.key.0) };
        if !current.is_null() {
            let current = Owned(current);
            if unsafe { CFEqual(current.0, self.value.0) } != 0 {
                return Ok(());
            }
        }
        if unsafe { SCDynamicStoreSetValue(self.store.0, self.key.0, self.value.0) } == 0 {
            return Err(io::Error::other(format!(
                "publish temporary TUN DNS resolver: SCError {}",
                unsafe { SCError() }
            )));
        }
        Ok(())
    }
    pub fn alive(interface: &str, pid: u32) -> bool {
        let Ok(name) = CString::new(interface) else {
            return false;
        };
        unsafe { libc::kill(pid as i32, 0) == 0 && libc::if_nametoindex(name.as_ptr()) != 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolver_property_list_parses_and_invalid_targets_fail_before_host_mutation() {
        assert!(Owned::plist("<plist version=\"1.0\"><dict><key>ServerAddresses</key><array><string>10.66.0.2</string></array><key>SupplementalMatchDomains</key><array><string></string></array></dict></plist>").is_ok());
        assert!(Resolver::install("utun6;invalid", "10.66.0.2", 42).is_err());
        assert!(Resolver::install("utun6", "127.0.0.1", 42).is_err());
    }
}
