# bb_exporter

Exporteur [Prometheus](https://prometheus.io/) pour les **Bbox de Bouygues Telecom**,
écrit en Rust. Développé et testé sur une **Bbox fibre Ultym** (Sagemcom F@st5696b,
Wi-Fi 7), il interroge l'API locale de la box (`https://mabbox.bytel.fr/api/v1`) et
expose les métriques au format Prometheus sur `/metrics`.

Inspiré du projet [sliiri/bb_exporter](https://github.com/sliiri/bb_exporter) (C#),
réécrit en Rust avec une image conteneur `FROM scratch`.

## Fonctionnement

L'exporteur tourne en continu :

1. une tâche de fond se connecte périodiquement à la box (POST `/api/v1/login`),
   collecte tous les endpoints puis se déconnecte ;
2. un serveur HTTP expose les dernières valeurs sur `/metrics`.

La fréquence de collecte (`BB_REFRESH_SECONDS`, 60 s par défaut) est découplée du
scrape Prometheus afin de ne pas surcharger la box.

## Configuration

Tout se configure par variables d'environnement (un fichier `.env` est chargé
automatiquement s'il est présent) :

| Variable             | Défaut                     | Description                                        |
| -------------------- | -------------------------- | -------------------------------------------------- |
| `BB_PASSWORD`        | *(requis)*                 | Mot de passe admin de la box, **URL-encodé**       |
| `BB_URL`             | `https://mabbox.bytel.fr`  | URL de base de la box                              |
| `BB_REFRESH_SECONDS` | `60`                       | Intervalle de collecte (min. 5 s)                  |
| `BB_LISTEN`          | `0.0.0.0:9100`             | Adresse d'écoute du serveur de métriques           |
| `BB_HOST_METRICS`    | `active`                   | Métriques par appareil : `active`, `all` ou `off`  |

### ⚠️ Encodage du mot de passe

Le mot de passe est inséré tel quel dans le corps `password=...` de la requête de
login (comportement identique au projet d'origine). Il doit donc être **URL-encodé**
si il contient des caractères spéciaux :

| Caractère | Encodage |
| --------- | -------- |
| `&`       | `%26`    |
| `?`       | `%3F`    |
| `=`       | `%3D`    |
| `+`       | `%2B`    |
| `%`       | `%25`    |
| espace    | `%20`    |

Exemple : le mot de passe `secret&pass?42` s'écrit `secret%26pass%3F42`.

## Build local

```sh
cargo build --release
BB_PASSWORD='secret%26pass%3F42' ./target/release/bb_exporter
curl http://127.0.0.1:9100/metrics
```

## Conteneur Docker (image `scratch`)

L'image finale est construite `FROM scratch` (≈ 9 Mo) : binaire statique musl, aucun
système de fichiers, aucun certificat CA requis (la box utilise un certificat
auto-signé que l'exporteur accepte explicitement).

```sh
docker build -t bb_exporter:latest .

docker run -d --name bb_exporter -p 9100:9100 \
  --add-host mabbox.bytel.fr:192.168.1.254 \
  -e BB_PASSWORD='secret%26pass%3F42' \
  bb_exporter:latest
```

### Image pré-construite (GitHub Container Registry)

Un workflow GitHub Actions ([`.github/workflows/docker.yml`](.github/workflows/docker.yml))
construit l'image **multi-arch (linux/amd64 + linux/arm64)** et la pousse sur le
GHCR du dépôt à chaque push sur la branche par défaut et sur chaque tag `v*` :

```sh
docker pull ghcr.io/<owner>/<repo>:latest
```

Tags publiés : `latest` (branche par défaut), la version sémantique sur tag `vX.Y.Z`,
et le SHA court de commit. Les pull requests déclenchent un build de validation
sans push.

### docker compose

Renseignez `BB_PASSWORD` dans un fichier `.env` (déjà ignoré par git), puis :

```sh
docker compose up -d --build
```

> `extra_hosts` / `--add-host` force la résolution de `mabbox.bytel.fr` vers l'IP LAN
> de la box (`192.168.1.254` par défaut). Adaptez si nécessaire.

## Configuration Prometheus

```yaml
scrape_configs:
  - job_name: bbox
    static_configs:
      - targets: ["bb_exporter:9100"]
```

## Dashboard Grafana

Un dashboard prêt à l'emploi est fourni : [`grafana/bbox_exporter.json`](grafana/bbox_exporter.json)
(testé sur **Grafana 13**, `schemaVersion` 42). Il couvre toutes les métriques :
vue d'ensemble (état, uptime, température, CPU), débit WAN/LAN/Wi-Fi, **volume de
données échangé (rx/tx) sur la période sélectionnée** (WAN et par interface, via
`increase(...[$__range])`), mémoire, radios Wi-Fi, inventaire des appareils
(débit de lien & signal), règles de firewall avec compteur « Utilisation »,
services et santé de l'exporteur.

**Import :** dans Grafana → *Dashboards* → *New* → *Import* → *Upload JSON file*,
puis sélectionnez votre source de données Prometheus (variable `Datasource`).

Le dashboard est portable (aucune source de données en dur) et propose deux
variables : `Datasource` (Prometheus) et `Instance` (filtre multi-box). Les
panneaux par appareil joignent `bbox_host_info` via le label `mac` pour afficher
les noms d'hôtes.

## Métriques exposées

| Métrique                          | Type  | Labels                  | Description                                |
| --------------------------------- | ----- | ----------------------- | ------------------------------------------ |
| `bbox_up`                         | gauge | —                       | Box opérationnelle (1)                     |
| `bbox_uptime_seconds`             | gauge | —                       | Uptime de la box                           |
| `bbox_boots_total`                | gauge | —                       | Nombre de démarrages                       |
| `bbox_info`                       | gauge | model, serial, firmware | Infos box (toujours 1)                     |
| `bbox_temperature_celsius`        | gauge | —                       | Température du SoC                          |
| `bbox_cpu_time_total`             | gauge | mode                    | Temps CPU cumulé (ticks)                   |
| `bbox_cpu_processes`              | gauge | state                   | Processus (created/running/blocked)        |
| `bbox_memory_bytes`               | gauge | type                    | Mémoire (total/free/cached/committed)      |
| `bbox_network_bytes_total`        | gauge | interface, direction    | Octets rx/tx par interface                 |
| `bbox_network_packets_total`      | gauge | interface, direction    | Paquets rx/tx                              |
| `bbox_network_errors_total`       | gauge | interface, direction    | Erreurs de paquets                         |
| `bbox_network_discards_total`     | gauge | interface, direction    | Paquets rejetés                            |
| `bbox_network_bandwidth`          | gauge | interface, direction    | Bande passante instantanée                 |
| `bbox_wan_internet_state`         | gauge | —                       | État de la connexion Internet              |
| `bbox_wan_link_up`                | gauge | —                       | Lien WAN actif                             |
| `bbox_wan_ip_up` / `bbox_wan_ip6_up` | gauge | —                    | IPv4 / IPv6 WAN actives                    |
| `bbox_wireless_radio_enabled`     | gauge | band                    | Radio Wi-Fi activée (2.4/5/6 GHz)          |
| `bbox_wireless_radio_state`       | gauge | band                    | État radio                                 |
| `bbox_wireless_channel`           | gauge | band                    | Canal courant                              |
| `bbox_wireless_bandwidth_mhz`     | gauge | band                    | Largeur de canal (MHz)                     |
| `bbox_hosts_total`                | gauge | —                       | Hôtes connus                               |
| `bbox_hosts_active`               | gauge | —                       | Hôtes actifs                               |
| `bbox_service_enabled`            | gauge | service                 | Service activé (valeur brute API)          |
| `bbox_service_status`             | gauge | service                 | Statut du service (valeur brute API)       |
| `bbox_firewall_rule_hits_total`   | gauge | id, rule, action, ip_protocol, protocols | Paquets ayant déclenché la règle (« Utilisation ») |
| `bbox_firewall_rule_enabled`      | gauge | id, rule                | Règle de firewall activée (1)              |
| `bbox_host_info`                  | gauge | mac, ip, hostname, link, band | Appareil connu (toujours 1)          |
| `bbox_host_ethernet_speed_mbps`   | gauge | mac                     | Vitesse de lien Ethernet négociée (Mbps)   |
| `bbox_host_wifi_rate_mbps`        | gauge | mac                     | Débit PHY Wi-Fi négocié (Mbps)             |
| `bbox_host_wifi_estimated_rate_mbps` | gauge | mac                  | Estimation du débit Wi-Fi courant (Mbps)   |
| `bbox_host_wifi_rssi_dbm`         | gauge | mac                     | Signal Wi-Fi (dBm)                         |
| `bbox_host_wifi_airtime_usage`    | gauge | mac, direction          | Usage airtime Wi-Fi (valeur brute API)     |
| `bbox_scrape_success`             | gauge | —                       | Succès de la dernière collecte             |
| `bbox_scrape_duration_seconds`    | gauge | —                       | Durée de la dernière collecte              |

Les interfaces réseau (`interface`) couvrent : `wan`, `lan`, `lan-port-N`,
`wifi-2.4`, `wifi-5`, `wifi-6`.

### Métriques par appareil

Les métriques `bbox_host_*` proviennent de l'endpoint `/api/v1/hosts`, **déjà
interrogé** pour les compteurs agrégés : leur ajout n'engendre donc **aucune
requête supplémentaire** vers la box. Les métriques numériques portent uniquement
le label `mac` (identifiant stable) ; les attributs descriptifs (ip, hostname,
lien, bande) sont dans `bbox_host_info`, à joindre par `mac` :

```promql
bbox_host_wifi_rate_mbps * on(mac) group_left(hostname, ip) bbox_host_info
```

Le périmètre se règle via `BB_HOST_METRICS` :

* `active` *(défaut)* — appareils actifs uniquement (cardinalité bornée) ;
* `all` — tous les hôtes connus (la liste accumule l'historique → cardinalité croissante) ;
* `off` — désactive ces métriques (les compteurs agrégés `bbox_hosts_total/active` restent).

> **Le total d'octets transféré par appareil n'est pas exposé par l'API de la Bbox**
> (aucun endpoint ne le fournit). Seules les caractéristiques de lien/débit
> instantané ci-dessus sont disponibles.

> Les compteurs cumulés (octets, paquets…) sont exposés comme `gauge` car l'API
> fournit des valeurs absolues. Utilisez `rate()` / `increase()` dans Prometheus
> comme pour un compteur classique.
