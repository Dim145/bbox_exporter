//! Registre Prometheus et mise à jour des métriques depuis un [`Snapshot`].

use anyhow::{Context, Result};
use prometheus::{Encoder, Gauge, GaugeVec, Opts, Registry, TextEncoder};

use crate::bbox::{Snapshot, Stats, Traffic};

/// Périmètre des métriques par appareil.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostMode {
    /// Aucune métrique par appareil.
    Off,
    /// Seulement les appareils actifs (cardinalité bornée, recommandé).
    Active,
    /// Tous les appareils connus de la box.
    All,
}

impl HostMode {
    /// Lit le mode depuis une variable d'environnement (`active` par défaut).
    pub fn from_env(value: Option<&str>) -> Self {
        match value.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            Some("off") | Some("false") | Some("none") | Some("0") => HostMode::Off,
            Some("all") => HostMode::All,
            _ => HostMode::Active,
        }
    }
}

/// Conteneur de toutes les métriques exposées.
pub struct Metrics {
    registry: Registry,

    // Méta / appareil
    up: Gauge,
    uptime: Gauge,
    boots: Gauge,
    info: GaugeVec,
    scrape_success: Gauge,
    scrape_duration: Gauge,

    // CPU / mémoire / température
    cpu_time: GaugeVec,       // mode
    cpu_processes: GaugeVec,  // state
    temperature: Gauge,
    memory: GaugeVec,         // type

    // Réseau (générique, labels interface/direction)
    net_bytes: GaugeVec,
    net_packets: GaugeVec,
    net_errors: GaugeVec,
    net_discards: GaugeVec,
    net_bandwidth: GaugeVec,

    // WAN
    wan_internet_state: Gauge,
    wan_link_up: Gauge,
    wan_ip_up: Gauge,
    wan_ip6_up: Gauge,

    // Wi-Fi radio (label band)
    radio_enabled: GaugeVec,
    radio_state: GaugeVec,
    radio_channel: GaugeVec,
    radio_bandwidth: GaugeVec,

    // Hôtes
    hosts_total: Gauge,
    hosts_active: Gauge,

    // Services (label service)
    service_enabled: GaugeVec,
    service_status: GaugeVec,

    // Règles de firewall (labels id/rule/action/ip_protocol)
    firewall_rule_hits: GaugeVec,
    firewall_rule_enabled: GaugeVec,

    // Par appareil (label mac ; détails descriptifs dans bbox_host_info)
    host_mode: HostMode,
    host_info: GaugeVec,
    host_ethernet_speed: GaugeVec,
    host_wifi_rate: GaugeVec,
    host_wifi_estimated_rate: GaugeVec,
    host_wifi_rssi: GaugeVec,
    host_wifi_airtime: GaugeVec,
}

/// Crée un `Gauge` simple et l'enregistre.
fn gauge(reg: &Registry, name: &str, help: &str) -> Result<Gauge> {
    let g = Gauge::new(name, help).with_context(|| format!("gauge {name}"))?;
    reg.register(Box::new(g.clone()))
        .with_context(|| format!("register {name}"))?;
    Ok(g)
}

/// Crée un `GaugeVec` avec labels et l'enregistre.
fn gauge_vec(reg: &Registry, name: &str, help: &str, labels: &[&str]) -> Result<GaugeVec> {
    let g = GaugeVec::new(Opts::new(name, help), labels)
        .with_context(|| format!("gaugevec {name}"))?;
    reg.register(Box::new(g.clone()))
        .with_context(|| format!("register {name}"))?;
    Ok(g)
}

impl Metrics {
    pub fn new(host_mode: HostMode) -> Result<Self> {
        let registry = Registry::new();
        let m = Self {
            up: gauge(&registry, "bbox_up", "Statut de la Bbox (1 = opérationnelle)")?,
            registry: registry.clone(),
            uptime: gauge(&registry, "bbox_uptime_seconds", "Uptime de la Bbox en secondes")?,
            boots: gauge(&registry, "bbox_boots_total", "Nombre de démarrages de la Bbox")?,
            info: gauge_vec(
                &registry,
                "bbox_info",
                "Informations sur la Bbox (toujours 1)",
                &["model", "serial", "firmware"],
            )?,
            scrape_success: gauge(
                &registry,
                "bbox_scrape_success",
                "1 si la dernière collecte auprès de la Bbox a réussi, 0 sinon",
            )?,
            scrape_duration: gauge(
                &registry,
                "bbox_scrape_duration_seconds",
                "Durée de la dernière collecte auprès de la Bbox",
            )?,
            cpu_time: gauge_vec(
                &registry,
                "bbox_cpu_time_total",
                "Temps CPU cumulé par mode (ticks)",
                &["mode"],
            )?,
            cpu_processes: gauge_vec(
                &registry,
                "bbox_cpu_processes",
                "Compteurs de processus",
                &["state"],
            )?,
            temperature: gauge(
                &registry,
                "bbox_temperature_celsius",
                "Température du SoC en degrés Celsius",
            )?,
            memory: gauge_vec(
                &registry,
                "bbox_memory_bytes",
                "Mémoire de la Bbox en octets par type",
                &["type"],
            )?,
            net_bytes: gauge_vec(
                &registry,
                "bbox_network_bytes_total",
                "Octets transférés par interface et direction",
                &["interface", "direction"],
            )?,
            net_packets: gauge_vec(
                &registry,
                "bbox_network_packets_total",
                "Paquets transférés par interface et direction",
                &["interface", "direction"],
            )?,
            net_errors: gauge_vec(
                &registry,
                "bbox_network_errors_total",
                "Erreurs de paquets par interface et direction",
                &["interface", "direction"],
            )?,
            net_discards: gauge_vec(
                &registry,
                "bbox_network_discards_total",
                "Paquets rejetés par interface et direction",
                &["interface", "direction"],
            )?,
            net_bandwidth: gauge_vec(
                &registry,
                "bbox_network_bandwidth",
                "Bande passante instantanée par interface et direction (unité Bbox)",
                &["interface", "direction"],
            )?,
            wan_internet_state: gauge(
                &registry,
                "bbox_wan_internet_state",
                "État de la connexion Internet WAN",
            )?,
            wan_link_up: gauge(&registry, "bbox_wan_link_up", "Lien WAN actif (1 = Up)")?,
            wan_ip_up: gauge(&registry, "bbox_wan_ip_up", "IPv4 WAN active (1 = Up)")?,
            wan_ip6_up: gauge(&registry, "bbox_wan_ip6_up", "IPv6 WAN active (1 = Up)")?,
            radio_enabled: gauge_vec(
                &registry,
                "bbox_wireless_radio_enabled",
                "Radio Wi-Fi activée par bande (1 = activée)",
                &["band"],
            )?,
            radio_state: gauge_vec(
                &registry,
                "bbox_wireless_radio_state",
                "État de la radio Wi-Fi par bande",
                &["band"],
            )?,
            radio_channel: gauge_vec(
                &registry,
                "bbox_wireless_channel",
                "Canal Wi-Fi courant par bande",
                &["band"],
            )?,
            radio_bandwidth: gauge_vec(
                &registry,
                "bbox_wireless_bandwidth_mhz",
                "Largeur de canal Wi-Fi courante (MHz) par bande",
                &["band"],
            )?,
            hosts_total: gauge(&registry, "bbox_hosts_total", "Nombre d'hôtes connus")?,
            hosts_active: gauge(&registry, "bbox_hosts_active", "Nombre d'hôtes actifs")?,
            service_enabled: gauge_vec(
                &registry,
                "bbox_service_enabled",
                "Service activé (valeur brute de l'API)",
                &["service"],
            )?,
            service_status: gauge_vec(
                &registry,
                "bbox_service_status",
                "Statut du service (valeur brute de l'API)",
                &["service"],
            )?,
            firewall_rule_hits: gauge_vec(
                &registry,
                "bbox_firewall_rule_hits_total",
                "Nombre de paquets ayant déclenché la règle de firewall (« Utilisation »)",
                &["id", "rule", "action", "ip_protocol", "protocols"],
            )?,
            firewall_rule_enabled: gauge_vec(
                &registry,
                "bbox_firewall_rule_enabled",
                "Règle de firewall activée (1 = activée)",
                &["id", "rule"],
            )?,
            host_mode,
            host_info: gauge_vec(
                &registry,
                "bbox_host_info",
                "Informations sur un appareil (toujours 1) ; à joindre par le label mac",
                &["mac", "ip", "hostname", "link", "band"],
            )?,
            host_ethernet_speed: gauge_vec(
                &registry,
                "bbox_host_ethernet_speed_mbps",
                "Vitesse de lien Ethernet négociée de l'appareil (Mbps)",
                &["mac"],
            )?,
            host_wifi_rate: gauge_vec(
                &registry,
                "bbox_host_wifi_rate_mbps",
                "Débit PHY Wi-Fi négocié de l'appareil (Mbps)",
                &["mac"],
            )?,
            host_wifi_estimated_rate: gauge_vec(
                &registry,
                "bbox_host_wifi_estimated_rate_mbps",
                "Estimation du débit Wi-Fi courant de l'appareil par la box (Mbps)",
                &["mac"],
            )?,
            host_wifi_rssi: gauge_vec(
                &registry,
                "bbox_host_wifi_rssi_dbm",
                "Niveau de signal Wi-Fi de l'appareil (dBm)",
                &["mac"],
            )?,
            host_wifi_airtime: gauge_vec(
                &registry,
                "bbox_host_wifi_airtime_usage",
                "Usage airtime Wi-Fi de l'appareil par direction (valeur brute de l'API)",
                &["mac", "direction"],
            )?,
        };
        Ok(m)
    }

    /// Renseigne les métriques réseau d'une interface depuis un bloc `Stats`.
    fn set_stats(&self, interface: &str, stats: &Stats) {
        self.set_traffic(interface, &stats.rx, &stats.tx);
    }

    /// Renseigne les métriques réseau depuis des compteurs rx/tx.
    fn set_traffic(&self, interface: &str, rx: &Traffic, tx: &Traffic) {
        for (dir, t) in [("rx", rx), ("tx", tx)] {
            self.net_bytes.with_label_values(&[interface, dir]).set(t.bytes);
            self.net_packets.with_label_values(&[interface, dir]).set(t.packets);
            self.net_errors.with_label_values(&[interface, dir]).set(t.packetserrors);
            self.net_discards.with_label_values(&[interface, dir]).set(t.packetsdiscards);
            self.net_bandwidth.with_label_values(&[interface, dir]).set(t.bandwidth);
        }
    }

    /// Met à jour les métriques par appareil selon le mode configuré.
    fn update_hosts(&self, hosts: &[crate::bbox::Host]) {
        // Réinitialise pour ne pas conserver d'appareils disparus / passés inactifs.
        self.host_info.reset();
        self.host_ethernet_speed.reset();
        self.host_wifi_rate.reset();
        self.host_wifi_estimated_rate.reset();
        self.host_wifi_rssi.reset();
        self.host_wifi_airtime.reset();

        if self.host_mode == HostMode::Off {
            return;
        }

        for h in hosts {
            if self.host_mode == HostMode::Active && h.active == 0.0 {
                continue;
            }
            if h.macaddress.is_empty() {
                continue;
            }
            let mac = h.macaddress.as_str();
            let hostname = fix_mojibake(&h.hostname);
            self.host_info
                .with_label_values(&[mac, &h.ipaddress, &hostname, &h.link, &h.wireless.band])
                .set(1.0);

            // Métriques Ethernet uniquement si lien filaire négocié.
            if h.ethernet.speed > 0.0 {
                self.host_ethernet_speed.with_label_values(&[mac]).set(h.ethernet.speed);
            }

            // Métriques Wi-Fi uniquement pour les appareils sur une bande radio.
            if !h.wireless.band.is_empty() {
                self.host_wifi_rate.with_label_values(&[mac]).set(h.wireless.rate);
                self.host_wifi_estimated_rate
                    .with_label_values(&[mac])
                    .set(h.wireless.estimated_rate);
                self.host_wifi_rssi.with_label_values(&[mac]).set(h.wireless.rssi0);
                self.host_wifi_airtime
                    .with_label_values(&[mac, "rx"])
                    .set(h.wireless.rx_usage);
                self.host_wifi_airtime
                    .with_label_values(&[mac, "tx"])
                    .set(h.wireless.tx_usage);
            }
        }
    }

    /// Met à jour l'ensemble des métriques depuis un instantané collecté.
    pub fn update(&self, snap: &Snapshot, duration_secs: f64) {
        self.scrape_duration.set(duration_secs);

        if let Some(d) = &snap.device {
            self.up.set(d.status);
            self.uptime.set(d.uptime);
            self.boots.set(d.numberofboots);
            // L'info-métrique ne porte qu'une série : on la (ré)affecte à 1.
            self.info.reset();
            self.info
                .with_label_values(&[&d.modelname, &d.serialnumber, &d.main.version])
                .set(1.0);
        }

        if let Some(c) = &snap.cpu {
            self.cpu_time.with_label_values(&["total"]).set(c.time.total);
            self.cpu_time.with_label_values(&["user"]).set(c.time.user);
            self.cpu_time.with_label_values(&["nice"]).set(c.time.nice);
            self.cpu_time.with_label_values(&["system"]).set(c.time.system);
            self.cpu_time.with_label_values(&["io"]).set(c.time.io);
            self.cpu_time.with_label_values(&["idle"]).set(c.time.idle);
            self.cpu_time.with_label_values(&["irq"]).set(c.time.irq);
            self.cpu_processes.with_label_values(&["created"]).set(c.process.created);
            self.cpu_processes.with_label_values(&["running"]).set(c.process.running);
            self.cpu_processes.with_label_values(&["blocked"]).set(c.process.blocked);
            // milli-°C -> °C
            self.temperature.set(c.temperature.main / 1000.0);
        }

        if let Some(mem) = &snap.mem {
            // L'API renvoie des kilo-octets.
            self.memory.with_label_values(&["total"]).set(mem.total * 1024.0);
            self.memory.with_label_values(&["free"]).set(mem.free * 1024.0);
            self.memory.with_label_values(&["cached"]).set(mem.cached * 1024.0);
            self.memory.with_label_values(&["committed"]).set(mem.committedas * 1024.0);
        }

        if let Some(w) = &snap.wan_ip {
            self.wan_internet_state.set(w.internet.state);
            self.wan_link_up.set(up_to_f64(&w.link.state));
            self.wan_ip_up.set(up_to_f64(&w.ip.state));
            self.wan_ip6_up.set(up_to_f64(&w.ip.ip6state));
        }

        if let Some(s) = &snap.wan_stats {
            self.set_stats("wan", s);
        }

        if let Some(lan) = &snap.lan {
            self.set_stats("lan", &lan.stats);
            for p in &lan.port {
                let iface = format!("lan-port-{}", p.index as i64);
                self.set_traffic(&iface, &p.rx, &p.tx);
            }
        }

        for (band, stats) in [
            ("2.4", &snap.wifi_24),
            ("5", &snap.wifi_5),
            ("6", &snap.wifi_6),
        ] {
            if let Some(s) = stats {
                self.set_stats(&format!("wifi-{band}"), s);
            }
        }

        if let Some(r) = &snap.wireless {
            for (band, b) in [("2.4", &r.b24), ("5", &r.b5), ("6", &r.b6)] {
                self.radio_enabled.with_label_values(&[band]).set(b.enable);
                self.radio_state.with_label_values(&[band]).set(b.state);
                self.radio_channel.with_label_values(&[band]).set(b.current_channel);
                self.radio_bandwidth.with_label_values(&[band]).set(b.current_bandwidth);
            }
        }

        if let Some(h) = &snap.hosts {
            self.hosts_total.set(h.list.len() as f64);
            self.hosts_active
                .set(h.list.iter().filter(|x| x.active != 0.0).count() as f64);
            self.update_hosts(&h.list);
        }

        if let Some(sv) = &snap.services {
            for (name, st) in [
                ("firewall", &sv.firewall),
                ("dhcp", &sv.dhcp),
                ("nat", &sv.nat),
                ("hotspot", &sv.hotspot),
                ("wifischeduler", &sv.wifischeduler),
                ("parentalcontrol", &sv.parentalcontrol),
            ] {
                self.service_enabled.with_label_values(&[name]).set(st.enable);
                self.service_status.with_label_values(&[name]).set(st.status);
            }
        }

        if let Some(rules) = &snap.firewall_rules {
            // Réinitialise pour éviter des séries fantômes si une règle disparaît.
            self.firewall_rule_hits.reset();
            self.firewall_rule_enabled.reset();
            for r in rules {
                let id = (r.id as i64).to_string();
                let desc = fix_mojibake(&r.description);
                self.firewall_rule_hits
                    .with_label_values(&[&id, &desc, &r.action, &r.ipprotocol, &r.protocols])
                    .set(r.utilisation);
                self.firewall_rule_enabled
                    .with_label_values(&[&id, &desc])
                    .set(r.enable);
            }
        }
    }

    pub fn set_scrape_success(&self, ok: bool) {
        self.scrape_success.set(if ok { 1.0 } else { 0.0 });
    }

    /// Rend les métriques au format texte d'exposition Prometheus.
    pub fn render(&self) -> Result<String> {
        let mut buf = Vec::new();
        TextEncoder::new()
            .encode(&self.registry.gather(), &mut buf)
            .context("encodage Prometheus")?;
        Ok(String::from_utf8(buf).context("UTF-8")?)
    }
}

/// L'API de la Bbox renvoie certaines chaînes en UTF-8 « double-encodé »
/// (chaque octet UTF-8 traité comme un codepoint, ex. `é` → `Ã©`). Si toute la
/// chaîne tient sur des octets (codepoints ≤ 0xFF) et que leur réinterprétation
/// donne de l'UTF-8 valide, on restitue la chaîne d'origine ; sinon on la
/// conserve telle quelle.
fn fix_mojibake(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| (c as u32) <= 0xFF) {
        let bytes: Vec<u8> = s.chars().map(|c| c as u8).collect();
        if let Ok(fixed) = String::from_utf8(bytes) {
            return fixed;
        }
    }
    s.to_string()
}

fn up_to_f64(state: &str) -> f64 {
    if state.eq_ignore_ascii_case("up") {
        1.0
    } else {
        0.0
    }
}
