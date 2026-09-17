//! Local interface addresses advertised for wildcard dashboard listeners.

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::ptr;

/// Return every configured interface address matching the wildcard family.
///
/// Loopback is retained because a wildcard listener is also reachable from
/// the machine that launched it. Unspecified addresses are not destinations.
pub(super) fn matching_addresses(bind_ip: IpAddr) -> Vec<IpAddr> {
    let mut addresses = BTreeSet::new();
    let mut head = ptr::null_mut();
    let family = match bind_ip {
        IpAddr::V4(_) => libc::AF_INET,
        IpAddr::V6(_) => libc::AF_INET6,
    };

    // SAFETY: `getifaddrs` initializes `head` on success. The guard frees the
    // returned linked list once, after every node has been inspected.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return vec![loopback_for(bind_ip)];
    }
    let _guard = IfAddrs(head);
    let mut current = head;
    while !current.is_null() {
        // SAFETY: every non-null node belongs to the live list owned by
        // `_guard`; `ifa_next` either points to another node or is null.
        let interface = unsafe { &*current };
        let address = interface.ifa_addr;
        if !address.is_null() && i32::from(unsafe { (*address).sa_family }) == family {
            // SAFETY: the family check proves `address` points to the matching
            // socket-address structure supplied by `getifaddrs`.
            let ip = unsafe { ip_from_sockaddr(address, family) };
            if !ip.is_unspecified() {
                addresses.insert(ip);
            }
        }
        current = interface.ifa_next;
    }

    if addresses.is_empty() {
        addresses.insert(loopback_for(bind_ip));
    }
    addresses.into_iter().collect()
}

unsafe fn ip_from_sockaddr(address: *const libc::sockaddr, family: i32) -> IpAddr {
    if family == libc::AF_INET {
        // SAFETY: the caller verified AF_INET and a non-null address.
        let raw = unsafe { (*(address.cast::<libc::sockaddr_in>())).sin_addr.s_addr };
        IpAddr::V4(Ipv4Addr::from(u32::from_be(raw)))
    } else {
        // SAFETY: the caller verified AF_INET6 and a non-null address.
        let octets = unsafe { (*(address.cast::<libc::sockaddr_in6>())).sin6_addr.s6_addr };
        IpAddr::V6(Ipv6Addr::from(octets))
    }
}

fn loopback_for(bind_ip: IpAddr) -> IpAddr {
    match bind_ip {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
    }
}

struct IfAddrs(*mut libc::ifaddrs);

impl Drop for IfAddrs {
    fn drop(&mut self) {
        // SAFETY: this is the exact list returned by the successful
        // `getifaddrs` call above and is freed exactly once.
        unsafe { libc::freeifaddrs(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::matching_addresses;

    #[test]
    fn interface_addresses_are_nonempty_sorted_destinations() {
        for wildcard in [
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        ] {
            let addresses = matching_addresses(wildcard);
            assert!(!addresses.is_empty());
            assert!(addresses.iter().all(|address| !address.is_unspecified()));
            assert!(addresses.windows(2).all(|pair| pair[0] < pair[1]));
            assert!(addresses
                .iter()
                .all(|address| address.is_ipv4() == wildcard.is_ipv4()));
        }
    }
}
