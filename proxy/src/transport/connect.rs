use futures::Future;
use tokio_connect;
use tokio_core::reactor::Handle;

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use http;

use connection;
use convert;
use dns;

#[derive(Debug, Clone)]
pub struct Connect {
    addr: SocketAddr,
    handle: Handle,
}

#[derive(Clone, Debug)]
pub struct HostAndPort {
    pub host: Host,
    pub port: u16,
}

#[derive(Clone, Debug)]
pub enum Host {
    DnsName(String),
    Ip(IpAddr),
}

#[derive(Clone, Copy, Debug)]
pub enum HostAndPortError {
    /// The port is missing.
    MissingPort,
}

#[derive(Debug, Clone)]
pub struct LookupAddressAndConnect {
    host_and_port: HostAndPort,
    dns_resolver: dns::Resolver,
    handle: Handle,
}

// ===== impl HostAndPort =====

impl<'a> convert::TryFrom<&'a http::uri::Authority> for HostAndPort {
    type Err = HostAndPortError;
    fn try_from(a: &http::uri::Authority) -> Result<Self, Self::Err> {
        let host = {
            let host = a.host();
            match IpAddr::from_str(host) {
                Err(_) => Host::DnsName(host.to_owned()),
                Ok(ip) => Host::Ip(ip),
            }
        };
        let port = a.port().ok_or_else(|| HostAndPortError::MissingPort)?;
        Ok(HostAndPort {
            host,
            port
        })
    }
}

impl<'a> From<&'a HostAndPort> for http::uri::Authority {
    fn from(a: &HostAndPort) -> Self {
        let s = match a.host {
            Host::DnsName(ref n) => format!("{}:{}", n, a.port),
            Host::Ip(ref ip) => format!("{}:{}", ip, a.port),
        };
        http::uri::Authority::from_str(&s).unwrap()
    }
}

// ===== impl Connect =====

impl Connect {
    /// Returns a `Connect` to `addr` and `handle`.
    pub fn new(addr: SocketAddr, handle: &Handle) -> Self {
        Self {
            addr,
            handle: handle.clone(),
        }
    }
}

impl tokio_connect::Connect for Connect {
    type Connected = connection::Connection;
    type Error = io::Error;
    type Future = connection::Connecting;

    fn connect(&self) -> Self::Future {
        connection::connect(&self.addr, &self.handle)
    }
}

// ===== impl LookupAddressAndConnect =====

impl LookupAddressAndConnect {
    pub fn new(
        host_and_port: HostAndPort,
        dns_resolver: dns::Resolver,
        handle: &Handle,
    ) -> Self {
        Self {
            host_and_port,
            dns_resolver,
            handle: handle.clone(),
        }
    }
}

impl tokio_connect::Connect for LookupAddressAndConnect {
    type Connected = connection::Connection;
    type Error = io::Error;
    type Future = Box<Future<Item = connection::Connection, Error = io::Error>>;

    fn connect(&self) -> Self::Future {
        let port = self.host_and_port.port;
        let handle = self.handle.clone();
        let host = self.host_and_port.host.clone();
        let c = self.dns_resolver
            .resolve_host(&self.host_and_port.host)
            .map_err(|_| {
                io::Error::new(io::ErrorKind::NotFound, "DNS resolution failed")
            })
            .and_then(move |ip_addr: IpAddr| {
                info!("DNS resolved {:?} to {}", host, ip_addr);
                let addr = SocketAddr::from((ip_addr, port));
                trace!("connect {}", addr);
                connection::connect(&addr, &handle)
            });
        Box::new(c)
    }
}
