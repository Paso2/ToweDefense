# Aethermaul TD — Game Design & Technical Specification (`GAME_SPEC.md`)

> **Version:** 1.0  
> **Zielplattform:** Eigenständige Windows-Desktop-Applikation (`x86_64` Single-Binary `.exe`, komplett ohne externe Asset-Abhängigkeiten lauffähig)  
> **Engine & Sprache:** Rust (Edition 2024) + Bevy Engine `0.20`  
> **Genre:** 3D Fantasy Mazing Tower Defense im klassischen Warcraft-3-Custom-Map-Stil (*Wintermaul*, *Green TD*, *Element TD*)

---

## 1. Vision & Design-Pfeiler

**Aethermaul TD** fängt das taktile, strategische Spielgefühl klassischer Warcraft-3-Tower-Defense-Maps in einer modernen, nativen 3D-Rust-Engine ein:

1. **Echtes WC3-Live-Mazing:** Freies Bauraster mit Checkpoints, sofortiger 3D-Pfadvorschau, Bau/Verkauf auch während aktiver Wellen, striktem Echtzeit-Anti-Block-Check und fairem Anti-Juggle-Schutz.
2. **5 eigenständige Fantasy-Fraktionen:** Nach klassischem Wintermaul-/Race-TD-Prinzip besitzt jede Fraktion eine komplett eigene Bauliste, eigene prozedurale 3D-Architektur und eine einzigartige **Spezialwährung** mit eigener Gewinnungsmechanik.
3. **Zero-Asset-Stand-Alone-`.exe`:** Alle 3D-Meshes, PBR-Materialien, Partikel, Animationen und Soundeffekte werden prozedural in Rust generiert. Standard-`.ron`-Dateien werden per `include_str!` direkt in die Binary eingebettet, können aber jederzeit durch externe Dateien im Ordner `./config/` neben der `.exe` überschrieben werden (volle Modbarkeit + 1-Datei-Portabilität).
4. **Visuelle Klarheit („Nicht überladen“):** Mindestens **80 % freie 3D-Spielfläche**, gedrosselte Popups, HP-Balken nur bei beschädigten Gegnern/Bossen und Status-Effekte direkt als lesbare 3D-Farb-/Partikel-Effekte am Modell statt überladener Icon-Stapel.
5. **Hohe Performance:** Datenorientiertes ECS + `O(1)`-Flow-Field-Pathfinding für konstant **≥ 60 FPS** bei **≥ 150 gleichzeitigen Gegnern, ≥ 200 Türmen und ≥ 300 Partikeln**.

---

## 2. Tech-Stack, Cargo-Konfiguration & Build-Pipeline

### 2.1 Verwendete Crates & `Cargo.toml`

Alle Versionen sind auf **Bevy `0.20`** (Rust Edition 2024) abgestimmt.

```toml
[package]
name = "aethermaul_td"
version = "0.1.0"
edition = "2024"
description = "3D Fantasy Mazing Tower Defense inspired by Warcraft 3 Custom Maps"

[features]
default = []
# Schnelles inkrementelles Linken während der Entwicklung:
# cargo run --features dev_fast
dev_fast = ["bevy/dynamic_linking"]
# Optionaler Live-ECS-Inspector (nur für Dev/Balancing, nicht im Release):
# cargo run --features "dev_fast dev_tools"
dev_tools = ["dep:bevy_egui", "dep:bevy-inspector-egui"]

[dependencies]
bevy = { version = "0.20", default-features = false, features = [
    "3d",
    "ui",
    "bevy_audio",
    "vorbis",           # Für optionale externe .ogg-Audio-Overrides
    "bevy_gizmos",      # 3D-Reichweitenringe & optionale Debug-Visualisierung
    "tonemapping_luts", # Sauberes HDR-Tonemapping + Bloom
    "multi_threaded",
    "sysinfo_plugin",   # Optional für FPS-/Frame-Time-Anzeige
] }
serde = { version = "1.0", features = ["derive"] }
ron = "0.12"
rand = "0.10"
smallvec = "1.16"
thiserror = "2.0"
directories = "6.0"

# Optionale Dev-Tools (nur mit --features dev_tools kompiliert)
bevy_egui = { version = "0.43", optional = true }
bevy-inspector-egui = { version = "0.38", optional = true }

# ---------------------------------------------------------------------------
# Fast-Compile Dev-Profil (Dependencies voll optimiert, eigener Code schnell)
# ---------------------------------------------------------------------------
[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3

# ---------------------------------------------------------------------------
# Performance-orientiertes Release-Profil für die Windows-.exe
# ---------------------------------------------------------------------------
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = "symbols"
panic = "abort"
```

### 2.2 Schneller Linker (`.cargo/config.toml`)

```toml
# Windows (MSVC) – nutzt rust-lld für deutlich schnellere Link-Zeiten
[target.x86_64-pc-windows-msvc]
linker = "rust-lld.exe"

# Linux (Dev-Host) – nutzt mold oder lld falls vorhanden
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=lld"]
```

### 2.3 Windows-Release-Build & Cross-Compilation

- **Konsole im Release unterdrücken:** In `src/main.rs` wird `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` gesetzt, sodass beim Doppelklick auf `aethermaul_td.exe` kein schwarzes Konsolenfenster erscheint.
- **Nativer Windows-Build (MSVC):**
  ```powershell
  cargo build --release --target x86_64-pc-windows-msvc
  # Ergebnis: target\x86_64-pc-windows-msvc\release\aethermaul_td.exe
  ```
- **Cross-Compile von Linux auf Windows (`cargo-xwin`):**
  ```bash
  rustup target add x86_64-pc-windows-msvc
  cargo install cargo-xwin
  cargo xwin build --release --target x86_64-pc-windows-msvc
  ```
- **Dev-Start mit Fast-Compile:**
  ```bash
  cargo run --features dev_fast
  # Mit Live-Inspector:
  cargo run --features "dev_fast dev_tools"
  ```

### 2.4 Datengetriebene `.ron`-Ladestrategie (Embedded + Override)

1. Beim Kompilieren werden alle Standard-Dateien aus `config/` über `include_str!` in ein zentrales `EmbeddedConfigs`-Modul eingebettet.
2. Beim Spielstart sucht der `ConfigLoader` zuerst nach `./config/<pfad>.ron` relativ zur ausführbaren Datei. Existiert eine externe Datei, überschreibt sie den eingebetteten Standardwert; bei Parse-Fehlern wird eine lesbare Fehlermeldung geloggt und sauber auf den eingebetteten Standard zurückgefallen.
3. Im `dev_tools`-Modus lädt Taste `F5` externe `.ron`-Dateien zur Laufzeit neu (`ConfigReloadMessage`), um Balancing-Werte ohne Neustart zu testen.

### 2.5 Performance-Budgets

| Metrik | Zielwert (Mid-Range GPU/CPU @ 1080p) |
|---|---|
| Framerate bei 150 Gegnern, 200 Türmen, 300 Partikeln | ≥ 60 FPS (Frame-Time ≤ 16.6 ms) |
| Flow-Field-Neuberechnung (alle Stages auf 30×22 Grid) | < 0.8 ms (synchron im Frame ohne Ruckler) |
| Anti-Block-Validierung beim Hover (pro Tile-Wechsel) | < 0.3 ms (nur bei Wechsel der Hover-Zelle) |
| Partikel- & Popup-Pool-Obergrenzen | Max. 350 aktive 3D-Partikel, max. 40 Popups |
| Simulations-Tickrate | Deterministisches `FixedUpdate` @ 60 Hz × `GameSpeed` |

---

## 3. Software- & ECS-Architektur

### 3.1 Verzeichnisstruktur

```text
ToweDefense/
├── Cargo.toml
├── .cargo/config.toml
├── GAME_SPEC.md
├── config/
│   ├── balance.ron                 # Globale Ökonomie-, Kampf- & Schwierigkeitswerte
│   ├── enemies.ron                 # Gegner-Archetypen & Boss-Fähigkeiten
│   ├── waves.ron                   # 30 handdesignte Wellen + Endless-Formel
│   ├── maps/
│   │   ├── lordaeron_bastion.ron   # Map 1: WC3-Grasland/Stein (28×20)
│   │   └── wintermaul_glacier.ron  # Map 2: Eis/Schnee (30×22)
│   └── factions/
│       ├── elemental_circle.ron    # Elementar-Zirkel (Elementar-Essenzen)
│       ├── citadel.ron             # Zitadelle (Handwerkskerne)
│       ├── dragon_clan.ron         # Drachenklan (Glutseelen)
│       ├── frostguard.ron          # Frostwacht / Eismenschen (Frostsplitter)
│       └── necropolis.ron          # Nekropolis / Untote (Seelen)
├── src/
│   ├── main.rs
│   ├── state.rs                    # AppState, WavePhase, GameSpeed
│   ├── config/                     # RON-Schemas, Loader, Validierung
│   ├── grid/                       # Grid-State, Flow-Field, Anti-Block, Anti-Juggle
│   ├── economy/                    # Gold, Zinsen, Passive Income, Fraktionswährungen
│   ├── combat/                     # Targeting, Projektile, Beams, Chains, Status-Effekte
│   ├── enemies/                    # Spawning, Movement (Ground/Air), Leak-Loop, Bosse
│   ├── visuals/                    # Prozedurale 3D-Meshes, PBR-Materialien, Animation, VFX
│   ├── audio/                      # Prozedurale PCM-SFX-Synthese + Playback-Limiter
│   ├── camera/                     # WC3-Isometrie-Kamera, Zoom/Pan/Orbit/Edge-Scroll
│   ├── ui/                         # Natives Bevy-UI WC3-HUD, Menüs, Minimap, Hotkeys
│   ├── persistence/                # settings.ron & profile_scores.ron via directories
│   └── dev_tools.rs                # Optionales egui-Overlay (#[cfg(feature = "dev_tools")])
└── tests/
    ├── pathfinding_tests.rs        # Anti-Block, No-Corner-Cutting, Multi-Stage-Flow-Field
    ├── combat_math_tests.rs        # WC3-Armor-Formel, Status-Stacking, Anti-Heal
    └── config_validation_tests.rs  # Validierung aller eingebetteten .ron-Dateien
```

### 3.2 Spielzustände (`States`) & Plugin-Struktur

- **`AppState`**: `MainMenu` → `MatchSetup` (Map, Fraktion, Schwierigkeit) → `InGame` ↔ `Paused` → `Victory` (mit Button *„Endless fortsetzen“*) / `GameOver`.
- **`WavePhase`** (Sub-State während `InGame`):
  - `BuildPrep`: Countdown bis zur nächsten Welle (`18.0 s`, erste Welle `25.0 s`). Early-Send jederzeit möglich.
  - `WaveActive`: Gegner spawnen und bewegen sich. Volles Live-Mazing aktiv.
- **`GameSpeed`** (Resource): `Paused (0.0)`, `Normal (1.0)`, `Fast (2.0)`, `Ultra (3.0)`. Skaliert das virtuelle `Time<Fixed>`-Delta für konsistente Simulation bei 1×/2×/3×.
- **Bevy 0.20 Event-Muster:** Gepufferte Domain-Nachrichten nutzen `Message` (`DamageApplied`, `EnemyKilled`, `EnemyLeaked`, `TowerBuilt`, `TowerSold`, `WaveCompleted`, `PlaySfx`); reaktive UI- & Lifecycle-Trigger nutzen Bevy `Observer` / `Event`.

---

## 4. Karten, Biome, Live-Mazing & Pathfinding

### 4.1 Karten-Datenmodell & v1.0-Maps

Jedes Tile hat eine Weltgröße von **`1.0 × 1.0`** Einheiten. Zulässige `TileKind`-Typen:
- `Buildable`: Freies Bauraster, begehbar für Bodengegner, bebaubar mit Türmen.
- `BlockedTerrain`: Unbebaubare und unpassierbare Naturhindernisse (Felsen, Säulen, Abgründe).
- `Spawn`: Startfeld (unbebaubar, mit leuchtendem 3D-Beschwörungsportal).
- `Checkpoint(u8)`: Offene, begehbare Wegpunkt-Zone (`2×2` Tiles, unbebaubar, markiert durch leuchtende Bodenrunen). Bodengegner und Flieger müssen Checkpoints in Reihenfolge `0 → 1 → ... → Goal` passieren.
- `Goal`: Zielportal (unbebaubar, leuchtender Kristall-Torbogen — löst den **WC3-Leak-Loop** aus).
- `Tower(Entity)`: Durch einen Turm belegtes `Buildable`-Tile (blockiert Bodenpfad).

#### Map 1: `Lordaeron-Bastion` (Standard-Map)
- **Raster:** `28 × 20` Tiles (`X: 0..27`, `Z: 0..19`)
- **Biom:** Klassisches WC3-Grasland & Lordaeron-Stein (sattes Moos-/Wiesengrün mit dezentem Schachbrett-Kontrast, warme goldene Sonne, graue Steinfassungen, prozedurale Tannen, Felsen & flackernde Fackeln außerhalb des Grids).
- **Pfad-Topologie (S-Kurve für maximale Maze-Tiefe):**
  - `Spawn`: `(2, 2)` (`2×2`)
  - `Checkpoint 0`: `(24, 3)` (`2×2`, rechts oben)
  - `Checkpoint 1`: `(3, 10)` (`2×2`, links Mitte)
  - `Checkpoint 2`: `(24, 16)` (`2×2`, rechts unten)
  - `Goal`: `(2, 17)` (`2×2`, links unten)
- **Taktisches Profil:** Lange offene Zonen zwischen den Checkpoints erlauben klassische WC3-Zickzack- und Spiral-Labyrinthe, sodass Türme im Zentrum mehrfach schießen können.

#### Map 2: `Wintermaul-Gletscher`
- **Raster:** `30 × 22` Tiles (`X: 0..29`, `Z: 0..21`)
- **Biom:** Eis & Schnee (kühles bläuliches Sonnenlicht, weiße/eisblaue Rasterkacheln, Kristall-Portale, verschneite Felsen & Eis-Spitzen am Rand — starker Kontrast für Feuer-/Blitz-Projektile).
- **Pfad-Topologie (Zwei-Quellen-Konvergenz):**
  - `Spawn A`: `(3, 2)` (`2×2`, oben links) & `Spawn B`: `(25, 2)` (`2×2`, oben rechts)
  - `Checkpoint 0`: `(14, 9)` (`2×2`, zentrale Eisbrücke — beide Ströme treffen sich hier)
  - `Checkpoint 1`: `(14, 17)` (`2×2`, Süd-Plattform)
  - `Goal`: `(14, 20)` (`2×2`, unteres Kristall-Tor)
- **Taktisches Profil:** Erfordert symmetrisches oder zusammenführendes Mazing im Norden und eine starke Kill-Zone im Zentrum.

### 4.2 Multi-Stage-Flow-Field-Pathfinding (Boden)

Anstatt pro Gegner A* zu rechnen, verwaltet die Engine für jede Ziel-Stage `s ∈ {0..num_checkpoints, Goal}` ein separates **Integration Field** (`dist: Vec<u16>`, `u16::MAX` = unerreichbar) und **Flow Vector Field** (`dir: Vec<Vec2>`):

- **Berechnung:** Rückwärts-Dijkstra ab allen Tiles des jeweiligen Ziel-Checkpoints/Goals mit Kosten:
  - Kardinal (N/S/O/W): `+10`
  - Diagonal (NO/NW/SO/SW): `+14`
- **No-Corner-Cutting (Pflicht für WC3-Mazing):** Ein diagonaler Übergang von `(x, z)` nach `(x+dx, z+dz)` ist **nur** erlaubt, wenn sowohl `(x+dx, z)` als auch `(x, z+dz)` begehbar sind. Gegner dürfen niemals durch schräg aneinanderstoßende Turm-Ecken schlüpfen.
- **Laufzeit-Bewegung (`O(1)` pro Gegner):**
  1. Gegner liest den normierten Richtungsvektor seines aktuellen Tiles in Stage `enemy.current_stage`.
  2. Leichte Zielpunkt-Glättung (Interpolation zum Mittelpunkt des nächsten Ziel-Tiles) + sanfte Separation (`radius = 0.28`), damit Schwärme natürlich wirken und nicht exakt aufeinander kleben.
  3. Betritt der Gegner die Zone seines aktuellen Checkpoints (`dist_to_cp <= 1.1`), inkrementiert `enemy.current_stage += 1`.
- **Luft-Navigation (`MovementDomain::Air`):**
  - Flieger schweben auf fester Höhe `AIR_ALTITUDE = 2.6` Welt-Einheiten und fliegen geradlinig von `Spawn → Checkpoint 0 → ... → Goal` (mit sanfter Kurven-Interpolation und leichter Höhen-Welle `±0.15`). Türme blockieren sie nicht.

### 4.3 Volles WC3-Live-Mazing: Anti-Block & Anti-Juggle

Bauen und Verkaufen sind **jederzeit** erlaubt — sowohl in `BuildPrep` als auch live in `WaveActive`. Um unfaire Exploits auszuschließen, gelten vier deterministische Regeln:

1. **Belegungs-Check:** Auf einem Tile, auf dem sich aktuell ein Bodengegner befindet (Kollisionsbox-Abstand `< 0.55` zum Tile-Zentrum), darf nicht gebaut werden.
2. **Strikter Echtzeit-Anti-Block-Check:** Beim Platzieren (und bereits beim Hover für die rote/grüne Vorschau) wird das Tile temporär blockiert und das Multi-Stage-Flow-Field probeweise berechnet. Der Bau ist **ungültig**, wenn:
   - ein Spawn seinen ersten Checkpoint nicht mehr erreicht, **oder**
   - ein Checkpoint den nächsten Checkpoint bzw. das Goal nicht mehr erreicht, **oder**
   - **irgendein aktuell lebender Bodengegner** auf einer Zelle steht, von der aus seine `current_stage` `u16::MAX` (unerreichbar) wäre.
3. **Reclaim-Cooldown (gegen Bau/Abriss-Ping-Pong):** Wird ein Turm während `WaveActive` verkauft, wird sein Tile für **`1.5 s`** als `ReclaimLocked` markiert (dezent rot schraffiert). In dieser Zeit kann dort nicht neu gebaut werden. In `BuildPrep` gibt es keinen Cooldown.
4. **Anti-Juggle-Enrage:** Wenn sich durch einen Live-Bau/Verkauf die Flow-Field-Distanz eines lebenden Bodengegners zu seinem aktuellen Checkpoint vergrößert (Umkehr/Umleitung), steigt sein `reroute_count`. Bei `reroute_count >= 3` innerhalb derselben Stage wird der Gegner für **`3.0 s` Enraged**:
   - `+40 %` Bewegungsgeschwindigkeit, kurzes rötliches Glühen, `6.0 s` interner Cooldown bevor erneut getriggert werden kann.
5. **Verkaufserstattung:**
   - Gebaut und verkauft in derselben `BuildPrep`-Phase: **100 %** Gold-Rückerstattung (ermöglicht stressfreies Planen des Labyrinths!).
   - Verkauf eines älteren Turms in `BuildPrep`: **90 %** des investierten Gesamtwerts (Gold; Spezialwährung wird **nicht** erstattet, um Tech-Swapping-Exploits zu verhindern — stattdessen klare Warnung im Tooltip).
   - Verkauf während `WaveActive`: **70 %** des investierten Goldwerts.

### 4.4 3D-Pfadvorschau

- Standardmäßig **nur im Bau-Modus** sichtbar (sowie manuell per Hotkey `P` umschaltbar, damit das Spielfeld im Kampf sauber bleibt).
- Durch Vorwärts-Folgen des Flow-Fields (`Spawn → CP0 → ... → Goal`) wird der kürzeste Bodenpfad als animiertes, schwebendes 3D-Lichtband (`y = 0.08`) mit laufenden Richtungspfeilen gerendert:
  - **Grün/Gold:** Valides Maze (mit Anzeige der Pfadlänge in Tiles als kleine Zahl am Maus-Tooltip, z. B. `Pfad: 64 Tiles (+8)`!).
  - **Rot:** Ungültig / blockiert (mit kurzem Grund: *„Blockiert Pfad“*, *„Gegner eingeschlossen“* oder *„Cooldown“*).
- **Air-Vorschau:** Wenn die nächste Welle Flieger enthält (oder ein Anti-Air-Turm platziert wird), erscheint zusätzlich eine dezente, gestrichelte cyanfarbene Fluglinie auf Höhe `y = 2.6`.

---

## 5. Ökonomie-System & WC3-Leak-Loop

### 5.1 Multi-Säulen-Gold-Ökonomie (Normal-Modus)

Alle Werte sind zentral in `config/balance.ron` hinterlegt:

| Einkommensquelle | Formel / Wert (Normal) | Auslöser |
|---|---|---|
| **Startgold** | `120 g` | Match-Start (reicht für 5–8 Tier-1-Türme als Startmaze) |
| **Kill-Bounty** | Gegner-spezifisch (`2 g` Ratte bis `120 g` Endboss) | Bei Tod des Gegners (**nur 1×**; kein Farming durch Loops) |
| **Passives Gold / Zeit** | `+2.0 g / Sek.` Grund-Income (+ Wirtschaftsgebäude) | Kontinuierlich während `WaveActive` (skaliert mit `GameSpeed`) |
| **Wellen-Bonus** | `25 + 6 × wave` Gold | Sofort beim vollständigen Abschluss einer Welle (oder Early-Send der nächsten) |
| **Zinsen (Interest)** | `7 %` auf aktuelles Gold (`floor(gold * 0.07)`), **Cap: `250 g` / Welle** | Am Ende jeder abgeschlossenen Welle / bei Early-Send (Zitadelle kann Rate & Cap steigern) |
| **Early-Send-Bonus** | `floor(remaining_prep_secs * (2.0 + 0.35 * next_wave))` Gold | Beim Klick auf *„Welle senden“* (`N`) vor Ablauf des Prep-Timers |

### 5.2 WC3-Leak-Loop & Gegner-Limit

Statt klassischer Spieler-HP nutzt **Aethermaul TD** den ikonischen WC3-Leak-Loop:

- **Portal-Warp:** Erreicht ein Gegner (Boden oder Luft) das Zielportal, wird er mit einem violetten Portal-Blitz zurück zum Spawn teleportiert:
  - Aktuelle HP und aktive Status-Effekte bleiben erhalten (`current_stage = 0`, `loop_count += 1`).
  - Kein mehrfacher Gold-Bounty (Gegner gibt sein Bounty erst beim tatsächlichen Tod).
  - Jede absolvierte Loop-Runde erhöht das **Slot-Gewicht** des Gegners nicht weiter, markiert ihn aber subtil mit einem lila Portal-Ring, sodass Spieler Leaker sofort erkennen.
- **Gegner-Limit & Gnadenfrist:**
  - Jeder lebende Gegner belegt Slots nach Typ:
    - Schwarm-Ratte / Mini-Schleim: `0.5` Slots
    - Standard / Fledermaus / Schleim / Vampir: `1.0` Slot
    - Gepanzerter Golem / Gargoyle: `2.0` Slots
    - Mini-Boss / Endboss: `5.0` Slots
  - `active_slots = ceil(sum_slot_weights)`
  - **Limit (Normal):** **`45` Slots** (`Schwer: 40`, `Albtraum: 35`).
  - Sobald `active_slots > limit`, startet der **`10.0 s` Gnadenfrist-Countdown** (akustisches Warn-Horn + rot pulsierende Leiste oben im HUD). Sinkt `active_slots <= limit`, bricht der Timer sofort ab und setzt sich zurück. Erreicht der Timer `0.0 s`, endet die Partie mit **Game Over**.

---

## 6. Die 5 Fraktionen, Spezialwährungen & Baulisten

Jede Fraktion wird vollständig über ihre eigene `.ron`-Datei beschrieben (`config/factions/*.ron`). Alle Fraktionen nutzen dieselbe ECS-Kampf- und Upgrade-Engine, besitzen aber eine **eigenständige Bauliste**, **eigene 3D-Architektur** und **eigene Spezialwährung**.  
**Mazing-Prinzip:** Es gibt keinen reinen „Nichts-tuenden Wall-Block“ — stattdessen besitzt jede Fraktion einen günstigen **Tier-1-Basisturm (`15–20 g`)**, mit dem das Labyrinth gebaut und später gezielt aufgewertet wird.

### Übersicht der 5 Fraktionen

| Fraktion | Spezialwährung | Gewinnungsmechanik | Passive Identität | Visueller Stil |
|---|---|---|---|---|
| **1. Elementar-Zirkel** | **Elementar-Essenzen** | Start: `1`; `+1` alle 3 Wellen (W3, W6, ...); `+1` pro Boss-Kill | Maximale Flexibilität durch Feuer/Wasser/Blitz & Dual-Elemente | Weiße Marmor-Obelisken, schwebende Runen-Ringe & leuchtende Elementarkristalle |
| **2. Zitadelle (Menschen)** | **Handwerkskerne** | Start: `1`; `+1` alle 2 Wellen; `+1` alle 4 Wellen pro *Schatzkammer* | Klassische Pfeil-/Tech-Upgrades (Schnellfeuer, Multishot, Schwer) + stärkste Gold-Wirtschaft | Mittelalterlicher grauer Stein, Holz-Zinnen, Messing-Ballisten & goldene Banner |
| **3. Drachenklan** | **Glutseelen** | Start: `4`; `+1` pro Kill an brennendem Gegner; `+1` pro 5 normalen Kills; `+15` pro Boss | Hohe Kosten, massiver AoE-Atem, Burn-DoT & Late-Game-Skalierung | Obsidian-Horte, Basalt-Säulen, animierte Drachenköpfe/Flügel & glühende Magma-Adern |
| **4. Frostwacht (Eismenschen)** | **Frostsplitter** | Start: `4`; `+1` pro Kill an verlangsamtem/gefrorenem Gegner; `+1` pro Freeze-Proc (1×/Gegner) | Maximale Maze-Kontrolle durch Slow, Einfrieren & *Shatter*-Crit (+80 % auf Gefrorene) | Kantige Eis-Monolithen, tiefblaues Runeneis, schwebende Schneekristalle |
| **5. Nekropolis (Untote)** | **Seelen** | Start: `6`; `+1` pro **jedem** Kill (Elite `+3`, Boss `+20`); verdoppelt im Radius der *Krypta* | Rüstungsbruch, starkes Gift/Anti-Heal & exponentielle Ressourcen-Skalierung | Dunkler Granit, Knochen-Spitzen, giftgrüne/violette Seelenfeuer-Ziggurate |

---

### 6.1 Fraktion 1: Elementar-Zirkel (`Elementar-Essenzen`)

*Kern-Fantasie: Klassisches Element TD — Basis-Pfeiler direkt in Feuer, Wasser oder Blitz verwandeln und anschließend entweder zum reinen Tier-2-Element oder zu einem der 3 Dual-Elemente fusionieren.*

| Turm | Tier / Pfad | Kosten | Ziel | Schaden | ASPD | Reichw. | Angriff & Spezial-Effekt |
|---|---|---|---|---|---|---|---|
| **Arkansplitter-Pfeiler** | T1 (Basis / Maze) | `15 g` | G+A | `11` | `1.1/s` | `3.8` | Schneller arkaner Bolzen; Upgrade-Basis für alle 3 Elemente |
| **Feuer-Obelisk** | T2 (← Basis) | `40 g + 1 Ess.` | G+A | `26` | `0.85/s` | `4.2` | Explosiver Feuerball (`1.2` Splash), `8 DPS` Burn (4 s) + **50 % Anti-Heal** |
| **Wasser-Brunnen** | T2 (← Basis) | `35 g + 1 Ess.` | G+A | `16` | `1.1/s` | `4.4` | Frost-Woge: **30 % Slow** (3.0 s) + `Chilled` (erhöht Blitz/Frost-Synergie) |
| **Blitz-Spule** | T2 (← Basis) | `45 g + 1 Ess.` | G+A | `24` | `1.0/s` | `4.6` | Sofortiger **Kettenblitz** (3 Ziele, -15 % je Sprung), **+25 % vs. Air** |
| **Inferno-Spire** | T3 Rein-Feuer (← Feuer) | `110 g + 1 Ess.` | G+A | `72` | `0.8/s` | `4.6` | Großer Splash (`1.7`), `24 DPS` Burn (5 s) + **70 % Anti-Heal** |
| **Gletscher-Quelle** | T3 Rein-Wasser (← Wasser) | `95 g + 1 Ess.` | G+A | `38` | `1.1/s` | `5.0` | AoE-Frostwelle (`1.5`), **48 % Slow** (3.5 s) + 20 % Chance auf `0.8 s Freeze` |
| **Gewitter-Pylon** | T3 Rein-Blitz (← Blitz) | `120 g + 1 Ess.` | G+A | `62` | `1.15/s` | `5.2` | **Kettenblitz (6 Ziele)**, **+35 % vs. Air**, 15 % Crit (1.8×) |
| **Dampf-Kondensator** | T3 Dual (Feuer+Wasser) | `105 g + 1 Ess.` | **Ground** | `48` | `0.9/s` | `4.2` | AoE-Dampfwolke (`1.9` Radius): **-25 % Slow** + **-25 Armor Shred** (4 s) |
| **Plasma-Kern** | T3 Dual (Feuer+Blitz) | `125 g + 1 Ess.` | G+A | `115` | `0.95/s` | `5.0` | Single-Target-Burst, **25 % Crit (2.0×)**; getötete Gegner explodieren für `20 %` Max-HP AoE |
| **Sturm-Säule** | T3 Dual (Wasser+Blitz) | `115 g + 1 Ess.` | G+A | `54` | `1.05/s` | `5.2` | 5-Ziel-Kettenblitz, **0.35 s Mini-Stun**, **-20 % Slow**, **+45 % vs. Air** |

---

### 6.2 Fraktion 2: Zitadelle / Menschen-Ingenieure (`Handwerkskerne`)

*Kern-Fantasie: Klassische WC3-Pfeiltürme mit Tech-Spezialisierungen (Schnellfeuer, Mehrfachschuss, schwere Geschosse), dezidierter Flak und Zins-/Gold-Wirtschaft.*

| Turm | Tier / Pfad | Kosten | Ziel | Schaden | ASPD | Reichw. | Angriff & Spezial-Effekt |
|---|---|---|---|---|---|---|---|
| **Wachturm (Pfeilturm)** | T1 (Basis / Maze) | `15 g` | G+A | `12` | `1.15/s` | `4.2` | Klassischer Pfeilschuss; verzweigt in alle 3 Tech-Pfade |
| **Schnellfeuer-Turm** | T2 Tech (← Wachturm) | `45 g + 1 Kern` | G+A | `11` | `3.2/s` | `4.4` | Hochfrequenz-Bolzen; jeder 4. Treffer auf dasselbe Ziel ignoriert `50 %` Rüstung |
| **Gatling-Balliste** | T3 (← Schnellfeuer) | `115 g + 2 Kerne` | G+A | `22` | `4.8/s` | `4.8` | Ramp-Up-Tempo bis `6.0/s` bei Dauerfeuer; vernichtet Einzelziele & Bosse |
| **Mehrfachschuss-Turm** | T2 Tech (← Wachturm) | `50 g + 1 Kern` | G+A | `18` | `1.2/s` | `4.6` | Feuert **3 Pfeile gleichzeitig** auf unterschiedliche Ziele (optimal vs. Schwärme & Fledermäuse) |
| **Salven-Bastion** | T3 (← Mehrfachschuss) | `125 g + 2 Kerne` | G+A | `36` | `1.35/s` | `5.2` | Feuert **5 Pfeile gleichzeitig**; Pfeile prallen 1× auf nahe Gegner ab |
| **Schwere Kanone** | T2 Tech (← Wachturm) | `55 g + 1 Kern` | **Ground** | `52` | `0.6/s` | `4.8` | Schweres Explosivgeschoss (`1.5` Splash), **-15 Armor Shred** (3 s) |
| **Belagerungs-Mörser** | T3 (← Kanone) | `135 g + 2 Kerne` | **Ground** | `145` | `0.55/s` | `6.0` | Riesiger Artillerie-Einschlag (`2.1` Splash), **-25 Armor Shred** + `0.4 s` Erschütterungs-Slow |
| **Flak-Harpyienjäger** | T2 Spezial (Standalone) | `50 g` | **AirOnly** | `42` | `1.4/s` | `6.5` | Flak-Splitter-Schuss (`1.4` Air-Splash); ultimativer Konter gegen Fledermäuse & Gargoyles |
| **Gilden-Schatzkammer** | Wirtschaft (max. 3) | `80 g` | *Kein* | `—` | `—` | `—` | `+3.0 g/s` Passiv-Income, `+1 %` Zinsrate (+40 g Zins-Cap), generiert alle 4 Wellen `+1 Kern` |

---

### 6.3 Fraktion 3: Drachenklan (`Glutseelen`)

*Kern-Fantasie: Gewaltige, drehende 3D-Drachenhorte mit Feuer-Atem, Burn-DoTs, Rüstungsschmelze und synergetischem Goldhort.*

| Turm | Tier / Pfad | Kosten | Ziel | Schaden | ASPD | Reichw. | Angriff & Spezial-Effekt |
|---|---|---|---|---|---|---|---|
| **Drachenbrut-Nest** | T1 (Basis / Maze) | `20 g` | G+A | `15` | `1.0/s` | `3.8` | Kleiner Feuerstoß + `4 DPS` Burn (3 s, liefert früh *Glutseelen*!) |
| **Rotdrachen-Hort** | T2 (← Brut-Nest) | `65 g + 8 Glut` | G+A | `38` | `0.85/s` | `4.4` | Flammenkegel-AoE (`1.5`), `16 DPS` Burn (4.5 s) + **60 % Anti-Heal** |
| **Ältester Feuerdrache** | T3 (← Rotdrache) | `170 g + 25 Glut` | G+A | `110` | `0.9/s` | `5.0` | Breiter Inferno-Atem (`2.0`), `40 DPS` Burn + **80 % Anti-Heal**; Burn springt bei Tod über |
| **Bronzedrachen-Spitze** | T2 (← Brut-Nest) | `70 g + 10 Glut` | G+A | `44` | `0.95/s` | `4.6` | Arkaner Sand-Atem: **-30 Armor Shred** (5 s) + **20 % Slow** |
| **Zeitdrachen-Sanctum** | T3 (← Bronzedrache) | `165 g + 25 Glut` | G+A | `95` | `1.1/s` | `5.2` | **-40 Armor Shred** + globale Verzerrungs-Aura (`3.5` Radius: Gegner `-25 %` Tempo) |
| **Sturmdrachen-Sitz** | T2 (← Brut-Nest) | `70 g + 10 Glut` | G+A | `36` | `1.25/s` | `5.2` | Blitz-Atem springt auf 3 Ziele, **+40 % vs. Air** |
| **Himmelswyrm-Gipfel** | T3 (← Sturmdrache) | `180 g + 28 Glut` | G+A | `88` | `1.4/s` | `6.0` | 5-Ziel-Sturm-Atem, **+55 % vs. Air**, ignoriert `40 %` Rüstung |
| **Goldhort-Schrein** | Support (max. 3) | `90 g + 12 Glut` | *Aura* | `—` | `—` | `3.6` | Benachbarte Drachen: `+15 %` Schaden; Gegner im Radius geben `+20 %` Kill-Bounty |

---

### 6.4 Fraktion 4: Frostwacht / Eismenschen (`Frostsplitter`)

*Kern-Fantasie: Taktisches Kontroll-Mazing — Gegner im Labyrinth durch gestapelte Kälte einfrieren und mit massiven Shatter-Crits zertrümmern.*

| Turm | Tier / Pfad | Kosten | Ziel | Schaden | ASPD | Reichw. | Angriff & Spezial-Effekt |
|---|---|---|---|---|---|---|---|
| **Eis-Pfeiler** | T1 (Basis / Maze) | `15 g` | G+A | `10` | `1.1/s` | `4.0` | Eissplitter: **18 % Slow** (2.5 s) + `1 Frost-Stack` (erzeugt früh *Frostsplitter*) |
| **Frostspeer-Werfer** | T2 (← Eis-Pfeiler) | `45 g + 5 Spl.` | G+A | `34` | `1.0/s` | `5.0` | Hoher Stichschaden; **Shatter: +80 % Schaden** gegen gefrorene Ziele |
| **Gletscher-Scharfschütze** | T3 (← Frostspeer) | `125 g + 18 Spl.` | G+A | `110` | `1.05/s` | `6.2` | **Shatter: +110 % Schaden** + löst bei Shatter-Kill eine `1.6`-Frost-Nova aus |
| **Schneesturm-Obelisk** | T2 (← Eis-Pfeiler) | `55 g + 6 Spl.` | G+A | `18` | `1.1/s` | `4.2` | AoE-Kälte (`1.6`), **38 % Slow** + `+2 Frost-Stacks` (bei **5 Stacks** → `1.2 s Freeze`!) |
| **Blizzard-Spire** | T3 (← Schneesturm) | `140 g + 22 Spl.` | G+A | `46` | `1.2/s` | `4.8` | Großer Blizzard (`2.1`), **55 % Slow**, `+3 Frost-Stacks` + **-15 Armor Shred** auf Gefrorene |
| **Eissplitter-Katapult** | T2 (← Eis-Pfeiler) | `60 g + 8 Spl.` | **Ground** | `42` | `0.75/s` | `4.8` | Einschlag zerberstet in 4 Eisscherben (`1.8` Splash); exzellent gegen Schwärme & Schleime |
| **Nordwind-Turm** | T2 Spezial (Standalone) | `50 g + 5 Spl.` | **AirOnly** | `38` | `1.3/s` | `6.2` | Starke Böe: **45 % Air-Slow** + `+2 Frost-Stacks` + `0.4` Einheiten Rückstoß auf Fledermäuse |

---

### 6.5 Fraktion 5: Nekropolis / Untote (`Seelen`)

*Kern-Fantasie: Aggressives Debuff- & Snowball-Gameplay — jeder einzelne Kill nährt die Seelen-Ökonomie für verheerende Flüche, Rüstungszerfall und Anti-Heal-Seuchen.*

| Turm | Tier / Pfad | Kosten | Ziel | Schaden | ASPD | Reichw. | Angriff & Spezial-Effekt |
|---|---|---|---|---|---|---|---|
| **Knochen-Spire** | T1 (Basis / Maze) | `15 g` | G+A | `12` | `1.2/s` | `4.0` | Schneller Knochenbolzen; günstige Mazing- & Upgrade-Basis |
| **Seuchen-Ziggurat** | T2 (← Knochen) | `50 g + 12 Seelen` | **Ground** | `24` | `0.9/s` | `4.2` | Giftwolke (`1.6`), `14 DPS` Gift (5 s) + **80 % Anti-Heal** (härtester Vampir-Konter!) |
| **Pest-Kathedrale** | T3 (← Seuchen) | `130 g + 35 Seelen` | **Ground** | `62` | `1.0/s` | `4.8` | Massive Seuche (`2.1`), `34 DPS` Gift + **95 % Anti-Heal**; kranke Ziele hinterlassen Giftpfütze (`3 s`) |
| **Obelisk der Verwesung** | T2 (← Knochen) | `55 g + 15 Seelen` | G+A | `32` | `1.1/s` | `4.6` | Kontinuierlicher Nekro-Strahl: stapelt **-8 Armor Shred/Treffer** bis **-35** (4 s) |
| **Lich-Thron** | T3 (← Verwesung) | `155 g + 45 Seelen` | G+A | `96` | `1.1/s` | `5.4` | Frost-Nekro-Nova (`1.7`): **-45 Armor Shred** + **35 % Slow** |
| **Banshee-Turm** | T2 (← Knochen) | `50 g + 12 Seelen` | G+A | `28` | `1.3/s` | `5.4` | Schrei-Welle (trifft 2 Ziele), **+40 % vs. Air** + **Fluch (`+20 %` eingehender Schaden, 4 s)** |
| **Seelen-Krypta** | Support (max. 3) | `70 g + 10 Seelen` | G+A | `14` | `0.8/s` | `4.5` | **Verdoppelt Seelen-Drops** im Radius `4.5`; alle 8 Kills im Radius: `+5 Bonus-Gold` |

---

## 7. Kampfsystem, Formeln & Zielerfassung

### 7.1 WC3-Rüstungs- & Schadensformel

Ein eingehender Treffer mit Basisschaden $D_{\text{base}}$ durchläuft folgende deterministische Pipeline:

1. **Situative Multiplikatoren:**
   - Anti-Air-Bonus (`1.0 + air_bonus` falls Ziel `Air`)
   - Shatter-Bonus (`1.0 + shatter_bonus` falls Ziel `Frozen`)
   - Kritischer Treffer (`crit_mult` mit Wahrscheinlichkeit `crit_chance`)
   - Fluch-/Aura-Multiplikator (`1.0 + curse_bonus + aura_dmg_bonus`)
2. **Effektive Rüstung:**
   $$A_{\text{eff}} = \text{clamp}\left(A_{\text{base}} \cdot (1 - \text{armor\_pen}) - A_{\text{shred}},\ -45.0,\ 300.0\right)$$
3. **Rüstungs-Faktor (WC3/MOBA-Formel, stabil auch im negativen Bereich):**
   $$M_{\text{armor}} = \begin{cases} \dfrac{100}{100 + A_{\text{eff}}} & \text{wenn } A_{\text{eff}} \ge 0 \\[10pt] 2 - \dfrac{100}{100 - A_{\text{eff}}} & \text{wenn } A_{\text{eff}} < 0 \end{cases}$$
   *(Beispiele: `+50` Armor → `0.667×` Schaden; `+100` Armor → `0.50×`; `-25` Armor → `1.20×`; `-45` Armor → `1.31×`.)*
4. **Endschaden:** $D_{\text{final}} = \max(1.0,\ D_{\text{base}} \cdot M_{\text{situativ}} \cdot M_{\text{armor}})$.

### 7.2 Status-Effekt-Regeln

| Status | Stacking-Regel | Obergrenze / Boss-Regel | 3D-Visual am Gegner |
|---|---|---|---|
| **Burn** | Höchster DPS gewinnt, Timer wird aufgefrischt | 50–80 % **Anti-Heal** | Aufsteigende orange Funken + Glühen |
| **Poison** | Bis 2 Quellen additiv, Timer refresh | 80–95 % **Anti-Heal** | Grüne Blasen-Partikel + Gift-Tint |
| **Slow** | Stärkster Slow gewinnt (kein additives 100 %-Lock) | Max. **65 %** (Bosse max. **35 %**) | Bläulicher Frost-Schimmer |
| **Freeze** | Ausgelöst bei 5 `Frost-Stacks` (`1.2 s` Stun, danach `3 s` Freeze-Immunität) | Bosse sind Stun-immun → erhalten stattdessen **+20 % Slow** & gelten für *Shatter* als `Frozen` | Halbtransparenter 3D-Eiskristall um den Gegner |
| **Armor Shred** | Höchster Einzelwert oder stapelbar (Verwesung) bis Cap | Max. **-45 Armor** | Violetter zerbrochener Ring um die Füße |
| **Curse** | Erneuert Timer (`4 s`) | Fest **+20 %** eingehender Schaden | Dunkelviolette Aura über dem Kopf |
| **Mini-Stun** | `0.35 s` Stopp (mit `2.5 s` Stun-Immunität danach) | Bosse immun | Kurzer weiß-blauer Blitz-Bogen |

### 7.3 Zielprioritäten (pro Turm & per Massenauswahl einstellbar)

- **`First` (Standard):** Gegner mit geringster Restdistanz zum Zielportal (inkl. verbleibender Checkpoints).
- **`Strongest`:** Höchste aktuelle HP.
- **`Weakest`:** Niedrigste aktuelle HP (ideal zum Finish von Regenerierern/Leakers).
- **`Closest`:** Geringste euklidische Distanz zum Turm.
- **`AirPriority`:** Bevorzugt fliegende Gegner (innerhalb derer `First`), fällt sonst auf Bodengegner zurück.

---

## 8. Gegner-Vielfalt, 30-Wellen-Fahrplan & Balancing

### 8.1 Gegner-Archetypen (`config/enemies.ron`)

| Archetyp | Domäne | Basis-Speed | Basis-Armor | Slot-Gew. | Kern-Mechanik & Prozedurales 3D-Modell |
|---|---|---|---|---|---|
| **Ratten-Schwarm** | Boden | `2.6` | `0` | `0.5` | Hohe Stückzahl, wenig HP; flinke graubraune Quader/Kegel mit wippender Schnauze & Schwanz |
| **Goblin-Plünderer** | Boden | `2.1` | `5` | `1.0` | Ausgewogener Standard-Gegner; grüner Torso, spitze Ohren, hoppelnder Gang |
| **Fledermaus** | **Luft** | `2.4` | `0` | `1.0` | Fliegt auf `y = 2.6` über das Maze; dunkler Körper mit sinus-animierten 3D-Flügeln |
| **Schleim-Splitter** | Boden | `1.5` | `5` | `1.0` | Spaltet sich bei Tod in **2 Mini-Schleime** (je `35 %` HP, `0.5` Slots, `1.9` Speed); grüner halbtransparenter Squash-&-Stretch-Blob |
| **Vampir-Lord** | Boden | `1.85` | `15` | `1.0` | **Regeneriert `4.5 %` Max-HP/s** (durch Burn/Poison massiv reduzierbar!); schwebender dunkler Umhang + rote Augen |
| **Steingolem** | Boden | `1.25` | `45` | `2.0` | Schwer gepanzerter Tank; massive graue Felsblöcke, stampfende Geh-Animation, leuchtende Kern-Runen |
| **Gargoyle** | **Luft** | `1.65` | `40` | `2.0` | **Gepanzerter Flieger** auf `y = 2.6`! Grauer Steinkörper mit großen schwingenden Steinflügeln |
| **Mini-Boss / Boss** | Boden (W25: Luft) | `1.15–1.35` | `30–65` | `5.0` | Große Krone/Aura, eigener Boss-HP-Balken oben im HUD + 1 aktive Boss-Fähigkeit (z. B. *Ratten-Ruf*, *Steinschild*, *Tempestoß*) |

### 8.2 30-Wellen-Fahrplan (Normal-Modus)

Die HP-Kurve skaliert sanft exponentiell mit klaren strategischen Tech-Checks alle 5 Wellen:

| Wellen | Zusammensetzung & Fokus | Beispiel-HP (pro Einheit) | Anzahl | Bounty / Unit | Design-Check |
|---|---|---|---|---|---|
| **W1–W3** | Goblins → Ratten-Schwarm → Goblins | `45` → `32` → `85` | `10 / 22 / 14` | `4 / 2 / 5 g` | Start-Maze & erste Fraktionswährung |
| **W4–W5** | Steingolem-Vorhut → **W5 Mini-Boss: Rattenkönig** (+ Ratten) | `220` / Boss `1.350` | `8` / `1 + 12` | `9 g` / `45 g` | Single-Target-DPS & erster Tier-2-Turm |
| **W6** | **Erste Luft-Welle: Fledermäuse** | `120` | `14` | `6 g` | **Anti-Air-Check 1** (früh im HUD angekündigt!) |
| **W7–W9** | Schleim-Splitter → Gepanzerte Goblins → Ratten-Flut | `190` → `280` → `115` | `12 / 16 / 34` | `8 / 7 / 3 g` | AoE-Splash & Split-Kontrolle |
| **W10** | **Boss: Uralter Schleim-Titan** (spaltet in 3 Riesen-Schleime) | `4.200` (+ 3× `900`) | `1` | `75 g` | **Phase-1-Meilenstein** (Essenz/Boss-Drop!) |
| **W11–W14** | Steingolems → Fledermäuse (schnell) → Schleime → Goblins+Ratten | `680`–`950` | `12–30` | `11–4 g` | Erste Tier-3 / Dual-Elemente online |
| **W15** | **Vampir-Regenerierer + Boss: Blutgraf** (`5 %` HP/s Regen) | `820` / Boss `9.500` | `14 + 1` | `12 g / 90 g` | **Anti-Heal- & Burst-Check** |
| **W16–W19** | Golem-Phalanx → Schleim-Sturm → Vampir-Vorhut → Fledermaus-Schwarm | `1.300`–`2.200` | `14–28` | `14–7 g` | Hoher Armor-Shred & Slow-Kontrolle |
| **W20** | **Gepanzerte Luft-Gargoyles + Boss: Obsidianschwinge (Air)** | `2.400` / Boss `16.000` | `12 + 1` | `18 g / 110 g` | **Anti-Air + Armor-Shred-Check!** |
| **W21–W24** | Gemischte Belagerungswellen (Boden + Luft gleichzeitig!) | `2.800`–`4.600` | `18–32` | `16–10 g` | Koordination von Maze-Kills & Air-Korridor |
| **W25** | **Doppel-Boss: Golem-Koloss (Ground) + Blut-Gargoyle (Air)** | `26.000` + `18.000` | `2 + 10` | `2× 80 g` | Split-Focus-Test (Ground & Air parallel) |
| **W26–W29** | Elite-Varianten aller Archetypen (Vampir-Ritter, Frost-Gargoyles, Mega-Schleime) | `5.500`–`9.200` | `20–36` | `20–12 g` | Voller Late-Game-Ausbau & Zins-Nutzung |
| **W30** | **Endboss: Aether-Dreadlord** (ruft alle 15 s Fledermäuse & Vampire, `60` Armor) | `68.000` | `1 + Escort` | `150 g` | **Sieg-Bedingung** → danach optional **Endless** |

### 8.3 Endless-Skalierung (ab Welle 31)

Ab Welle $w > 30$ werden gemischte Wellen deterministisch generiert (jedes 5. Level enthält einen skalierten Boss):
- **HP:** $\text{HP}(w) = \text{HP}_{30,\text{archetyp}} \cdot 1.18^{(w - 30)}$
- **Armor:** $\text{Armor}(w) = \text{Armor}_{\text{base}} + 3 \cdot (w - 30)$
- **Speed:** $\min(\text{Speed}_{\text{base}} \cdot (1 + 0.015 \cdot (w - 30)),\ 1.45 \cdot \text{Speed}_{\text{base}})$
- **Bounty:** $\text{Bounty}(w) = \text{round}(\text{Bounty}_{30} \cdot 1.05^{(w - 30)})$

### 8.4 Schwierigkeitsgrade (`config/balance.ron`)

| Parameter | Normal | Schwer | Albtraum |
|---|---|---|---|
| Startgold | `120 g` | `100 g` | `90 g` |
| Gegner-HP-Multiplikator | `1.00×` | `1.25×` | `1.55×` |
| Gegner-Rüstungs-Bonus | `+0` | `+8` | `+16` |
| Kill-Bounty-Multiplikator | `1.00×` | `0.95×` | `0.90×` |
| Gegner-Limit (Slots) | `45` | `40` | `35` |
| Score-Multiplikator | `1.0×` | `1.5×` | `2.25×` |

---

## 9. Prozedurale 3D-Visuals, Kamera & Audio

### 9.1 WC3-Isometrie-Kamera

- **Projektion:** Perspektivische Kamera (`FOV = 42°`) für spürbare 3D-Tiefe bei klassischem RTS-Look.
- **Winkel:** Fester Standard-Pitch `52°` (bei Zoom sanft zwischen `44°` nah und `60°` fern gekoppelt), Yaw standardmäßig `0°` (optional per Mittelklick-Orbit begrenzt rotierbar ±35° oder frei 360° per Setting).
- **Zoom:** Mausrad smooth-interpoliert zwischen Höhe `12.0` (Nahaufnahme auf Türme/Kämpfe) und `36.0` (vollständiger Map-Überblick).
- **Steuerung:** `WASD` / Pfeiltasten, Mittelklick-Drag-Pan, optionales Edge-Scrolling (in Settings an/aus, Default: aus für Fenstermodus-Komfort), `Home` / `C` zentriert die Kamera.

### 9.2 Prozedurales 3D-Baukasten-System (Zero External Assets)

Alle 3D-Modelle entstehen beim Start einmalig in einem `ProceduralMeshCache` (`Assets<Mesh>` + `Assets<StandardMaterial>`), sodass Hunderte Instanzen effizient gebatcht werden:

- **Terrain & Rand-Deko:**
  - Pro Tile ein leicht abgeschrägter Quader (`Bevel`-Optik) mit dezenter Biom-Farbvarianz (Schachbrett-Muster für perfekte Rasterlesbarkeit).
  - Checkpoints als abgesenkte Steinplatten mit rotierendem emissivem Runen-Ring; Spawn/Goal als 3D-Stein-Torbögen mit pulsierendem Portal-Kern.
  - Prozedurale Rand-Deko außerhalb des Baurasters (gestapelte Kegel-Tannen, verzerrte Ikosaeder-Felsen, Fackeln mit flackerndem `PointLight` – streng begrenzt auf max. 6 Lichter für GPU-Performance).
- **Türme (mehrteilige Hierarchie):**
  - `BaseEntity` (Fundament/Sockel — fraktionsspezifisch: Marmor, Stein, Obsidian, Eisblock, Knochen-Sockel).
  - `TurretHead` (Child-Entity, rotiert flüssig per `Quat::slerp` zum aktuellen Ziel, inkl. kurzem Rückstoß-Kick beim Schuss!).
  - `ElementalAccent` (Child-Entity: rotierender emissiver Kristall, Flammen-Kugel, drehende Blitz-Ringe oder schlagende Drachenflügel — macht Element & Tier sofort aus der Vogelperspektive erkennbar).
- **Gegner (prozedurale 3D-Animation ohne externe Rigs):**
  - Kompositions-Hierarchie aus Primitiven (Körper + Kopf + Gliedmaßen/Flügel) angetrieben von einem leichtgewichtigen `ProceduralAnim`-Component (`Hop`, `WingFlap`, `SquashStretch`, `FloatSway`, `HeavyStomp`).
  - Schattenwurf für alle Türme und Bodengegner; Flieger werfen einen weichen, dunklen Bodenschatten direkt bei `(x, 0.03, z)`, damit ihre exakte 2D-Position über dem Labyrinth jederzeit eindeutig ablesbar ist!
- **Beleuchtung & Post-Processing:**
  - 1 `DirectionalLight` mit dynamischen kaskadierten Schatten (`CascadeShadowConfigBuilder`), Biom-Ambient-Licht, Bevy Contact Shadows und dezentes HDR-Bloom (`intensity = 0.15`) für Elementar-Magie & Portale.

### 9.3 Prozedurale SFX-Synthese in Rust

Beim Start erzeugt `ProceduralAudioPlugin` alle Soundeffekte als `44.1 kHz` Mono-PCM-WAV-Buffer im RAM (`bevy::audio::AudioSource`) — komplett ohne externe Audiodateien:

| Sound-Event | Synthese-Rezept |
|---|---|
| `ArrowShot` | Kurzes Bandpass-Rauschen (35 ms) + fallender Dreieck-Sweep `900 → 300 Hz` |
| `FireCast` / `Explosion` | Tiefpass-gefiltertes braunes Rauschen (180–300 ms) + Sub-Sinus `110 → 35 Hz` |
| `IceHit` / `Freeze` | Hohe metallische FM-Glöckchen (`1800 / 2750 Hz`) mit klirrender kurzer Decay-Hüllkurve |
| `LightningZap` | Hart geclipptes Rechteck + Frequenz-Jitter (`400–2400 Hz`, 90 ms) |
| `BuildTower` | Dumpfer Holz-/Stein-Thud (`150 → 55 Hz`, 90 ms) + kurzer Klick |
| `UpgradeTower` | Aufsteigender Fantasy-Dur-Dreiklang (Arpeggio `C5-E5-G5`, 220 ms) |
| `GoldCoin` | Zweiton-Metall-Ping (`1318 Hz → 1760 Hz`, 110 ms) |
| `LimitWarning` | Tiefes Fantasy-Kriegshorn (gedämpfte Sägezahn-Quinte `110 + 165 Hz`, 600 ms) |

- **Anti-Spam-Schutz:** Pro Sound-Typ gilt ein Polyphonie-Cooldown (`45–80 ms`) + zufällige Pitch-Variation (`±6 %`) sowie Master-/SFX-Lautstärkeregler und Mute-Button.
- **Optionaler Override:** Liegt eine Datei `./audio/<event_name>.ogg` neben der `.exe`, wird sie automatisch statt des synthetisierten Samples verwendet.

---

## 10. WC3-HUD, „Nicht überladen“-Regeln & Steuerung

### 10.1 Schlankes WC3-Screen-Layout (Natives Bevy UI)

```text
┌────────────────────────────────────────────────────────────────────────────┐
│ [Gold: 245g (+17g Zins)] [Essenzen: 3] | [Welle 12/30] [Gegner: 18/45] | [N: Senden (+32g)] [|| 1x 2x 3x] [⚙] │  ← max. 44 px
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│                                                                            │
│                      FREIES 3D-SCHLACHTFELD (≥ 80 %)                       │
│                   (Boss-HP-Balken oben mittig NUR bei Boss)                │
│                                                                            │
│                                                                            │
├──────────────────┬───────────────────────────────────┬─────────────────────┤
│  [2D Minimap]    │  (Nur sichtbar bei Selektion)     │  3×3 Command-Card   │
│  + Nächste Welle │  Turmname | DPS | Kills           │  [Q] [W] [E]        │
│    (Icon + Typ)  │  Ziel: [First ▾]  [Verkaufen (X)] │  [A] [S] [D]        │
└──────────────────┴───────────────────────────────────┴─────────────────────┘
```

### 10.2 Anti-Clutter-Leitlinien („Nicht überladen“)

1. **Kontextuelles Bottom-Center-Panel:** Der Selektions-Inspektor wird **ausgeblendet**, wenn nichts selektiert ist — das gibt den unteren mittleren Bildbereich komplett frei.
2. **HP-Balken:** 3D-HP-Balken über Gegnern erscheinen **nur**, wenn `hp < max_hp` oder es ein Boss ist (per Taste `Alt` oder Setting jederzeit alle anzeigbar).
3. **Keine Status-Icon-Flut:** Status-Effekte werden primär als gut lesbare 3D-Effekte am Gegner gerendert (Frost-Eis, Feuer-Funken, Gift-Blasen, lila Shred-Ring); detaillierte Zahlen sieht man erst, wenn man den Gegner anklickt.
4. **Gedrosselte Popups:** Schwebende Zahlen erscheinen nur für **Gold-Kills**, **Crits** und **Shatter-Procs** (nicht für jeden 10-Schaden-Schnellfeuer-Pfeil!), begrenzt auf einen Pool von max. `40` gleichzeitig und in Settings komplett abschaltbar.
5. **Saubere Tooltips:** Kompakter Hover-Tooltip direkt über der Command-Card mit Kosten, DPS, Zieltyp (`Boden` / `Luft` / `Boden+Luft`) und 1-Zeilen-Effektbeschreibung.

### 10.3 Steuerung & Hotkeys

| Eingabe | Aktion |
|---|---|
| `Linksklick` | Turm platzieren (im Bau-Modus) / Turm oder Gegner selektieren |
| `Doppelklick` auf Turm | Selektiert alle eigenen Türme desselben Typs (für Massen-Upgrades & Zielpriorität!) |
| `Shift + Linksklick` | **Multi-Build**: Bleibt nach Platzierung im Bau-Modus für schnelles Mazing |
| `Rechtsklick` / `Esc` | Bau-Modus abbrechen / Selektion aufheben / Pause-Menü |
| `Q W E / A S D` | Command-Card: Bauen (wenn nichts gewählt) oder direkt Upgraden (wenn Turm gewählt) |
| `X` | Selektierten Turm verkaufen (zeigt Erstattungsbetrag) |
| `T` | Zielpriorität des/der selektierten Türme durchschalten (`First → Strongest → Weakest → Closest → Air`) |
| `N` | Nächste Welle sofort senden (**Early-Send-Bonus**) |
| `Space` | Spiel pausieren / fortsetzen |
| `1` / `2` / `3` | Spielgeschwindigkeit `1×` / `2×` / `3×` |
| `P` | 3D-Pfadvorschau dauerhaft ein-/ausschalten |
| `Alt` | Alle Gegner-HP-Balken einblenden |
| `WASD` / Pfeile / Mausrad / Mittelklick | Kamera verschieben, zoomen & schwenken (`C` = Kamera zentrieren) |

---

## 11. Datengetriebene `.ron`-Schemas (Beispiele)

### 11.1 `config/balance.ron`

```ron
BalanceConfig(
    prep_time_first_wave_secs: 25.0,
    prep_time_secs: 18.0,
    passive_gold_per_sec: 2.0,
    wave_bonus_base: 25,
    wave_bonus_per_wave: 6,
    interest_rate: 0.07,
    interest_cap: 250,
    early_send_base_per_sec: 2.0,
    early_send_wave_scale: 0.35,
    sell_refund_same_prep: 1.0,
    sell_refund_prep: 0.90,
    sell_refund_active_wave: 0.70,
    reclaim_cooldown_secs: 1.5,
    anti_juggle_reroute_threshold: 3,
    anti_juggle_enrage_secs: 3.0,
    anti_juggle_speed_mult: 1.40,
    limit_grace_period_secs: 10.0,
    max_slow_normal: 0.65,
    max_slow_boss: 0.35,
    max_armor_shred: 45.0,
    difficulties: {
        Normal: (start_gold: 120, hp_mult: 1.00, armor_add: 0.0, bounty_mult: 1.00, max_slots: 45, score_mult: 1.0),
        Hard:   (start_gold: 100, hp_mult: 1.25, armor_add: 8.0, bounty_mult: 0.95, max_slots: 40, score_mult: 1.5),
        Nightmare: (start_gold: 90, hp_mult: 1.55, armor_add: 16.0, bounty_mult: 0.90, max_slots: 35, score_mult: 2.25),
    },
)
```

### 11.2 Beispiel-Turm in `config/factions/elemental_circle.ron`

```ron
FactionConfig(
    id: "elemental_circle",
    display_name: "Elementar-Zirkel",
    currency_name: "Elementar-Essenzen",
    starting_currency: 1,
    currency_rule: EveryNWavesAndBoss(every_waves: 3, wave_amount: 1, boss_amount: 1),
    primary_color: (0.28, 0.68, 0.95),
    accent_color: (0.95, 0.82, 0.35),
    root_buildable_towers: ["arcane_pillar"],
    towers: {
        "storm_pillar": TowerConfig(
            name: "Sturm-Säule",
            hotkey: "E",
            tier: 3,
            gold_cost: 115,
            special_cost: 1,
            target_mask: GroundAndAir,
            damage: 54.0,
            attack_speed: 1.05,
            range: 5.2,
            air_bonus_mult: 0.45,
            attack_kind: ChainLightning(bounces: 5, decay_per_bounce: 0.12, jump_range: 2.6),
            on_hit_effects: [
                Slow(percent: 0.20, duration: 2.5),
                MiniStun(duration: 0.35, immunity_window: 2.5),
            ],
            upgrades_to: [],
            mesh_recipe: StormPillar,
        ),
    },
)
```

---

## 12. Lokale Persistenz (Highscores & Einstellungen)

- **Speicherpfad:** Standardmäßig plattformkonform über `directories::ProjectDirs::from("com", "Aethermaul", "AethermaulTD")` (`%APPDATA%\Aethermaul\AethermaulTD\` unter Windows). Existiert eine Datei `portable.flag` im Ordner der `.exe`, wird stattdessen `./save/` genutzt.
- **`settings.ron`:** Auflösung/Vollbild, VSync, Schattenqualität, Master-/SFX-Lautstärke, Edge-Scrolling, Schadens-Popups ein/aus, dauerhafte HP-Balken, Kamera-Sensitivität.
- **`profile_scores.ron`:** Gespeicherte Bestleistungen je `(MapId, FactionId, Difficulty)` — höchste erreichte Welle, höchster Endless-Score, Siege, gesamte Kills & schnellster 30-Wellen-Clear.

---

## 13. Umsetzungs-Roadmap (Inkrementelle Meilensteine)

1. **Meilenstein 1 — Fundament & Live-Mazing-Prototyp**
   - Cargo-Setup (Bevy `0.20`, `dev_fast`, Release-Profil), `.ron`-Config-Loader mit `include_str!` + Override.
   - `Lordaeron-Bastion` 3D-Grid, WC3-Kamera, Multi-Stage-Flow-Field (mit No-Corner-Cutting), Echtzeit-Anti-Block, Reclaim-Cooldown & 3D-Pfadvorschau.
   - Unit-Tests für Pathfinding & Anti-Block.
2. **Meilenstein 2 — Kern-Gameplay & Erste Fraktion**
   - Kampfsystem (Rüstungsformel, Zielprioritäten, Projektile/Splash/Chain, Status-Effekte).
   - Kompletter **Elementar-Zirkel** (alle 10 Türme inkl. Dual-Elemente), vollständige Gold-Ökonomie (Zinsen, Passive Income, Early-Send) & WC3-Leak-Loop mit Limit-Countdown.
   - Alle Gegner-Archetypen (inkl. Air-Fledermäuse, Schleim-Splitter, Vampir-Regen, Gargoyles & Bosse) + erste 15 Wellen.
3. **Meilenstein 3 — Alle 5 Fraktionen, 30 Wellen & Endless**
   - Implementierung der 4 weiteren Fraktionen (*Zitadelle*, *Drachenklan*, *Frostwacht*, *Nekropolis*) inkl. individueller Spezialwährungs-Mechaniken und Wirtschafts-/Aura-Türme.
   - Kompletter 30-Wellen-Plan + Endless-Generator + 3 Schwierigkeitsgrade.
   - 2. Map (`Wintermaul-Gletscher` im Schnee-/Eis-Biom).
4. **Meilenstein 4 — Prozedurales 3D-Polish, WC3-HUD & Audio**
   - Ausgearbeitete prozedurale 3D-Turm- & Gegner-Modelle (drehende Köpfe, Elementar-Optik, Animationen, Flieger-Bodenschatten, gepoolte Partikel).
   - Finales natives Bevy-UI WC3-HUD (Top-Bar, 3×3-Command-Card mit Hotkeys, Kontext-Inspektor, 2D-Minimap, Shift-Multi-Build, Doppelklick-Massenauswahl).
   - Prozedurale PCM-SFX-Synthese + optionales `dev_tools`-egui-Overlay.
5. **Meilenstein 5 — Persistenz, Balancing-Pass & Windows-Release-`.exe`**
   - `settings.ron` & `profile_scores.ron` + Hauptmenü/Auswahlbildschirm/Victory-&-GameOver-Screens.
   - Automatisierter Performance-Stress-Test (150+ Gegner, 200+ Türme) & Balancing-Feinschliff.
   - Signierter/optimierter Single-Binary-Windows-Release-Build (`aethermaul_td.exe`) + Verifikation ohne externe Dateien.
