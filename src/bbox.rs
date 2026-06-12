//! Client HTTP pour l'API de la Bbox (Bouygues Telecom).
//!
//! L'API est exposée sur `https://mabbox.bytel.fr/api/v1`. L'authentification se
//! fait par un POST `password=...` sur `/login` qui pose un cookie de session
//! (géré automatiquement par le `cookie_store` de reqwest).
//!
//! Particularités observées sur la Bbox Ultym (Sagemcom F@st5696b) :
//!   * chaque endpoint renvoie un tableau JSON à un seul élément (`[{...}]`) ;
//!   * de nombreux compteurs sont renvoyés tantôt en nombre, tantôt en chaîne
//!     (`"bytes":"1094546274933"`). On désérialise donc en `f64` de façon
//!     tolérante via [`flex_f64`].

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer};

/// Désérialise un nombre qu'il soit encodé en JSON number ou en JSON string.
fn flex_f64<'de, D>(d: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(f64),
        Str(String),
    }
    match NumOrStr::deserialize(d)? {
        NumOrStr::Num(n) => Ok(n),
        NumOrStr::Str(s) => {
            let t = s.trim();
            if t.is_empty() {
                Ok(0.0)
            } else {
                t.parse::<f64>().map_err(serde::de::Error::custom)
            }
        }
    }
}

/// Désérialise un champ texte qu'il soit encodé en JSON string ou en JSON
/// number (la Bbox renvoie p.ex. la bande Wi-Fi tantôt `"2.4"`, tantôt `5`).
fn flex_string<'de, D>(d: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StrOrNum {
        Str(String),
        Num(serde_json::Number),
    }
    Ok(match StrOrNum::deserialize(d)? {
        StrOrNum::Str(s) => s,
        StrOrNum::Num(n) => n.to_string(),
    })
}

// ----------------------------------------------------------------------------
// Modèles de données
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DeviceResp {
    pub device: Device,
}

#[derive(Debug, Deserialize)]
pub struct Device {
    #[serde(default, deserialize_with = "flex_f64")]
    pub status: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub numberofboots: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub uptime: f64,
    #[serde(default)]
    pub modelname: String,
    #[serde(default)]
    pub serialnumber: String,
    #[serde(default)]
    pub main: FirmwareVersion,
}

#[derive(Debug, Default, Deserialize)]
pub struct FirmwareVersion {
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct CpuResp {
    pub device: CpuDevice,
}

#[derive(Debug, Deserialize)]
pub struct CpuDevice {
    pub cpu: Cpu,
}

#[derive(Debug, Deserialize)]
pub struct Cpu {
    pub time: CpuTime,
    #[serde(default)]
    pub process: CpuProcess,
    #[serde(default)]
    pub temperature: CpuTemperature,
}

#[derive(Debug, Default, Deserialize)]
pub struct CpuTime {
    #[serde(default, deserialize_with = "flex_f64")]
    pub total: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub user: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub nice: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub system: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub io: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub idle: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub irq: f64,
}

#[derive(Debug, Default, Deserialize)]
pub struct CpuProcess {
    #[serde(default, deserialize_with = "flex_f64")]
    pub created: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub running: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub blocked: f64,
}

#[derive(Debug, Default, Deserialize)]
pub struct CpuTemperature {
    /// Température en milli-degrés Celsius (ex: 65308 => 65.308 °C).
    #[serde(default, deserialize_with = "flex_f64")]
    pub main: f64,
}

#[derive(Debug, Deserialize)]
pub struct MemResp {
    pub device: MemDevice,
}

#[derive(Debug, Deserialize)]
pub struct MemDevice {
    pub mem: Mem,
}

/// Valeurs en kilo-octets.
#[derive(Debug, Default, Deserialize)]
pub struct Mem {
    #[serde(default, deserialize_with = "flex_f64")]
    pub total: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub free: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub cached: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub committedas: f64,
}

/// Statistiques rx/tx génériques (LAN, WAN, Wi-Fi).
#[derive(Debug, Default, Deserialize)]
pub struct Traffic {
    #[serde(default, deserialize_with = "flex_f64")]
    pub bytes: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub packets: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub packetserrors: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub packetsdiscards: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub bandwidth: f64,
}

#[derive(Debug, Default, Deserialize)]
pub struct Stats {
    #[serde(default)]
    pub rx: Traffic,
    #[serde(default)]
    pub tx: Traffic,
}

#[derive(Debug, Deserialize)]
pub struct WanIpStatsResp {
    pub wan: WanIpStatsWan,
}
#[derive(Debug, Deserialize)]
pub struct WanIpStatsWan {
    pub ip: WanIpStatsIp,
}
#[derive(Debug, Deserialize)]
pub struct WanIpStatsIp {
    pub stats: Stats,
}

#[derive(Debug, Deserialize)]
pub struct WanIpResp {
    pub wan: WanIp,
}
#[derive(Debug, Deserialize)]
pub struct WanIp {
    #[serde(default)]
    pub internet: WanState,
    #[serde(default)]
    pub ip: WanIpAddr,
    #[serde(default)]
    pub link: WanLink,
}
#[derive(Debug, Default, Deserialize)]
pub struct WanState {
    #[serde(default, deserialize_with = "flex_f64")]
    pub state: f64,
}
#[derive(Debug, Default, Deserialize)]
pub struct WanIpAddr {
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub ip6state: String,
}
#[derive(Debug, Default, Deserialize)]
pub struct WanLink {
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct LanStatsResp {
    pub lan: LanStats,
}
#[derive(Debug, Deserialize)]
pub struct LanStats {
    #[serde(default)]
    pub stats: Stats,
    #[serde(default)]
    pub port: Vec<LanPort>,
}
#[derive(Debug, Default, Deserialize)]
pub struct LanPort {
    #[serde(default, deserialize_with = "flex_f64")]
    pub index: f64,
    #[serde(default)]
    pub rx: Traffic,
    #[serde(default)]
    pub tx: Traffic,
}

#[derive(Debug, Deserialize)]
pub struct WirelessStatsResp {
    pub wireless: WirelessStatsWireless,
}
#[derive(Debug, Deserialize)]
pub struct WirelessStatsWireless {
    pub ssid: WirelessSsid,
}
#[derive(Debug, Deserialize)]
pub struct WirelessSsid {
    #[serde(default)]
    pub stats: Stats,
}

#[derive(Debug, Deserialize)]
pub struct WirelessResp {
    pub wireless: Wireless,
}
#[derive(Debug, Deserialize)]
pub struct Wireless {
    #[serde(default)]
    pub radio: WirelessRadio,
}
#[derive(Debug, Default, Deserialize)]
pub struct WirelessRadio {
    #[serde(default, rename = "24")]
    pub b24: WirelessBand,
    #[serde(default, rename = "5")]
    pub b5: WirelessBand,
    #[serde(default, rename = "6")]
    pub b6: WirelessBand,
}
#[derive(Debug, Default, Deserialize)]
pub struct WirelessBand {
    #[serde(default, deserialize_with = "flex_f64")]
    pub enable: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub state: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub current_channel: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub current_bandwidth: f64,
}

#[derive(Debug, Deserialize)]
pub struct HostsResp {
    pub hosts: Hosts,
}
#[derive(Debug, Deserialize)]
pub struct Hosts {
    #[serde(default)]
    pub list: Vec<Host>,
}
#[derive(Debug, Deserialize)]
pub struct Host {
    #[serde(default, deserialize_with = "flex_f64")]
    pub active: f64,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub ipaddress: String,
    #[serde(default)]
    pub macaddress: String,
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub ethernet: HostEthernet,
    #[serde(default)]
    pub wireless: HostWireless,
}

#[derive(Debug, Default, Deserialize)]
pub struct HostEthernet {
    /// Vitesse de lien négociée en Mbps (ex. 10000 = 10 GbE).
    #[serde(default, deserialize_with = "flex_f64")]
    pub speed: f64,
}

#[derive(Debug, Default, Deserialize)]
pub struct HostWireless {
    /// Bande radio ("2.4", "5", "6"…) ; vide si l'hôte n'est pas en Wi-Fi.
    /// Renvoyée tantôt en chaîne, tantôt en nombre selon la bande.
    #[serde(default, deserialize_with = "flex_string")]
    pub band: String,
    /// Débit PHY négocié (Mbps).
    #[serde(default, deserialize_with = "flex_f64")]
    pub rate: f64,
    /// Estimation du débit courant par la box (Mbps).
    #[serde(default, rename = "estimatedRate", deserialize_with = "flex_f64")]
    pub estimated_rate: f64,
    /// Usage airtime en réception (valeur brute de l'API).
    #[serde(default, rename = "rxUsage", deserialize_with = "flex_f64")]
    pub rx_usage: f64,
    /// Usage airtime en émission (valeur brute de l'API).
    #[serde(default, rename = "txUsage", deserialize_with = "flex_f64")]
    pub tx_usage: f64,
    /// Niveau de signal (dBm) ; renvoyé tantôt en chaîne ("-29"), tantôt en nombre.
    #[serde(default, deserialize_with = "flex_f64")]
    pub rssi0: f64,
}

#[derive(Debug, Deserialize)]
pub struct FirewallRulesResp {
    pub firewall: FirewallRulesWrapper,
}
#[derive(Debug, Deserialize)]
pub struct FirewallRulesWrapper {
    #[serde(default)]
    pub rules: Vec<FirewallRule>,
}
#[derive(Debug, Deserialize)]
pub struct FirewallRule {
    #[serde(default, deserialize_with = "flex_f64")]
    pub id: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub enable: f64,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub protocols: String,
    #[serde(default)]
    pub ipprotocol: String,
    /// Nombre de paquets ayant déclenché la règle (compteur « Utilisation »).
    #[serde(default, deserialize_with = "flex_f64")]
    pub utilisation: f64,
}

#[derive(Debug, Deserialize)]
pub struct ServicesResp {
    pub services: Services,
}
#[derive(Debug, Default, Deserialize)]
pub struct Services {
    #[serde(default)]
    pub firewall: ServiceState,
    #[serde(default)]
    pub dhcp: ServiceState,
    #[serde(default)]
    pub nat: ServiceState,
    #[serde(default)]
    pub hotspot: ServiceState,
    #[serde(default)]
    pub wifischeduler: ServiceState,
    #[serde(default)]
    pub parentalcontrol: ServiceState,
}
#[derive(Debug, Default, Deserialize)]
pub struct ServiceState {
    #[serde(default, deserialize_with = "flex_f64")]
    pub enable: f64,
    #[serde(default, deserialize_with = "flex_f64")]
    pub status: f64,
}

/// Instantané complet d'une collecte : chaque champ est optionnel car un
/// endpoint peut échouer indépendamment sans faire échouer toute la collecte.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub device: Option<Device>,
    pub cpu: Option<Cpu>,
    pub mem: Option<Mem>,
    pub wan_ip: Option<WanIp>,
    pub wan_stats: Option<Stats>,
    pub lan: Option<LanStats>,
    pub wireless: Option<WirelessRadio>,
    pub wifi_24: Option<Stats>,
    pub wifi_5: Option<Stats>,
    pub wifi_6: Option<Stats>,
    pub hosts: Option<Hosts>,
    pub services: Option<Services>,
    pub firewall_rules: Option<Vec<FirewallRule>>,
}

// ----------------------------------------------------------------------------
// Client
// ----------------------------------------------------------------------------

pub struct BboxClient {
    http: reqwest::Client,
    base_url: String,
    /// Mot de passe déjà URL-encodé, prêt à être inséré tel quel dans le corps
    /// `password=...` (voir README).
    password: String,
}

impl BboxClient {
    pub fn new(base_url: String, password: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            // La Bbox présente un certificat auto-signé : on ne peut pas le valider.
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .context("construction du client HTTP")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            password,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }

    /// Authentification : renvoie l'en-tête `Cookie` de session à réutiliser sur
    /// les requêtes suivantes (on n'utilise pas de cookie store global afin que
    /// chaque cycle de collecte soit indépendant).
    async fn login(&self) -> Result<String> {
        let resp = self
            .http
            .post(self.url("/login"))
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            // Corps envoyé verbatim : le mot de passe est déjà URL-encodé.
            .body(format!("password={}", self.password))
            .send()
            .await
            .context("requête de login")?;
        if !resp.status().is_success() {
            anyhow::bail!("échec du login : HTTP {}", resp.status());
        }
        // Concatène les paires `name=value` de tous les Set-Cookie reçus.
        let cookie = resp
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .filter_map(|c| c.split(';').next())
            .collect::<Vec<_>>()
            .join("; ");
        if cookie.is_empty() {
            anyhow::bail!("login : aucun cookie de session reçu");
        }
        Ok(cookie)
    }

    async fn logout(&self, cookie: &str) {
        let _ = self
            .http
            .post(self.url("/logout"))
            .header(reqwest::header::COOKIE, cookie)
            .body("")
            .send()
            .await;
    }

    /// Récupère un endpoint renvoyant `[{...}]` et désérialise le premier élément.
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str, cookie: &str) -> Result<T> {
        let resp = self
            .http
            .get(self.url(path))
            .header(reqwest::header::COOKIE, cookie)
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        let status = resp.status();
        let text = resp.text().await.with_context(|| format!("corps {path}"))?;
        if !status.is_success() {
            anyhow::bail!("GET {path} : HTTP {status}");
        }
        let mut items: Vec<T> = serde_json::from_str(&text)
            .with_context(|| format!("désérialisation {path}"))?;
        items
            .drain(..)
            .next()
            .with_context(|| format!("réponse vide pour {path}"))
    }

    /// Effectue un cycle complet : login, collecte de tous les endpoints, logout.
    ///
    /// Un échec sur un endpoint individuel est journalisé et n'interrompt pas la
    /// collecte des autres métriques.
    pub async fn collect(&self) -> Result<Snapshot> {
        let cookie = self.login().await?;

        let mut snap = Snapshot::default();

        macro_rules! try_get {
            ($field:ident, $ty:ty, $path:expr, $map:expr) => {
                match self.get::<$ty>($path, &cookie).await {
                    Ok(v) => snap.$field = Some($map(v)),
                    Err(e) => tracing::warn!("collecte {} échouée : {:#}", $path, e),
                }
            };
        }

        try_get!(device, DeviceResp, "/device", |r: DeviceResp| r.device);
        try_get!(cpu, CpuResp, "/device/cpu", |r: CpuResp| r.device.cpu);
        try_get!(mem, MemResp, "/device/mem", |r: MemResp| r.device.mem);
        try_get!(wan_ip, WanIpResp, "/wan/ip", |r: WanIpResp| r.wan);
        try_get!(wan_stats, WanIpStatsResp, "/wan/ip/stats", |r: WanIpStatsResp| r
            .wan
            .ip
            .stats);
        try_get!(lan, LanStatsResp, "/lan/stats", |r: LanStatsResp| r.lan);
        try_get!(wireless, WirelessResp, "/wireless", |r: WirelessResp| r
            .wireless
            .radio);
        try_get!(wifi_24, WirelessStatsResp, "/wireless/24/stats", |r: WirelessStatsResp| r
            .wireless
            .ssid
            .stats);
        try_get!(wifi_5, WirelessStatsResp, "/wireless/5/stats", |r: WirelessStatsResp| r
            .wireless
            .ssid
            .stats);
        try_get!(wifi_6, WirelessStatsResp, "/wireless/6/stats", |r: WirelessStatsResp| r
            .wireless
            .ssid
            .stats);
        try_get!(hosts, HostsResp, "/hosts", |r: HostsResp| r.hosts);
        try_get!(services, ServicesResp, "/services", |r: ServicesResp| r.services);
        try_get!(firewall_rules, FirewallRulesResp, "/firewall/rules", |r: FirewallRulesResp| r
            .firewall
            .rules);

        self.logout(&cookie).await;
        Ok(snap)
    }
}
