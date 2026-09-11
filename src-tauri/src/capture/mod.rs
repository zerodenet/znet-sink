//! System capture lifecycle. OS proxy and Zero TUN retain separate ownership.
pub mod connection;
pub(crate) mod shutdown;
pub mod system_proxy;
pub(crate) mod tun;
pub(crate) mod tun_restore;
