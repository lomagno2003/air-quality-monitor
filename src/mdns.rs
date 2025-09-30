// src/mdns.rs
#![allow(dead_code)]

use core::net::{IpAddr, Ipv4Addr};
use embassy_net::{udp, IpAddress, Ipv4Address, Stack};
use embassy_time::{Duration, Instant, Timer};
use esp_hal_mdns::MdnsQuery;
use log::info;

/// Super-small wrapper that discovers a DNS-SD service (e.g. "_mqtt._tcp.local")
/// and returns the **host IP** from its SRV/A records.
pub struct MdnsFacade;

impl MdnsFacade {
    pub const fn new() -> Self { Self }

    /// Browse `service_name` and return the first IPv4 address found.
    /// Retries for ~5s total; loop/extend to taste.
    pub async fn query_service<'s>(
        &self,
        service_name: &'static str,                     // e.g. "_mqtt._tcp.local"
        stack: &'static Stack<'s>,
    ) -> (IpAddr, u16) {
        loop {
            if stack.is_link_up() {
                info!("Network is up.");
                if stack.config_v4().is_some() { 
                    info!("DHCP configured!");
                    break;
                }
                info!("DHCP not configured yet. Waiting..");
            } else {
                info!("Network is down. Waiting..");
            }
            Timer::after_millis(400).await;
        }

        let _ = stack.join_multicast_group(IpAddress::v4(224, 0, 0, 251));

        let mut rx_meta: [udp::PacketMetadata; 4] = [udp::PacketMetadata::EMPTY; 4];
        let mut rx_buff:  [u8; 1024] = [0; 1024];
        let mut tx_meta: [udp::PacketMetadata; 4] = [udp::PacketMetadata::EMPTY; 4];
        let mut tx_buff:  [u8; 512]  = [0; 512];

        let mut sock = udp::UdpSocket::new(
            *stack,
             &mut rx_meta,  &mut rx_buff,
             &mut tx_meta,  &mut tx_buff,
        );
        sock.bind(5353).ok();

        let mut q = MdnsQuery::new(
            service_name,
            1000, // resend interval ms
            || Instant::now().as_millis() as u64,
        );
        let mdns_peer = (Ipv4Address::new(224, 0, 0, 251), 5353);
        let deadline  = Instant::now() + Duration::from_millis(5000);
        let mut rx = [0u8; 1024];

        loop {
            info!("Querying mDNS for service {:?}", service_name);
            if let Some(pkt) = q.should_send_mdns_packet() {
                let _ = sock.send_to(pkt, mdns_peer).await;
            }
            if let Ok((n, _peer)) = sock.recv_from(&mut rx).await {
                let (ip_v4, port, _instance) = q.parse_mdns_query(&rx[..n], None);
                
                if port != 0 && ip_v4 != [0, 0, 0, 0] {
                    return (IpAddr::V4(Ipv4Addr::new(ip_v4[0], ip_v4[1], ip_v4[2], ip_v4[3])), port);
                }
            }
            if Instant::now() >= deadline {
                // no result yet: back off a bit, then extend the window
                Timer::after(Duration::from_millis(250)).await;
            }
            Timer::after(Duration::from_millis(40)).await;
        }
    }
}